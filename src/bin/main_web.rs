// src/bin/main_web.rs

#[macro_use] extern crate rocket;
#[macro_use] extern crate lazy_static;
extern crate rand;
extern crate maud;
extern crate tokio_postgres;

use dotenv::dotenv;
use rocket::fairing::AdHoc;
use rocket::tokio::sync::broadcast;
use deadpool_postgres::{Config as DpPgConfig, Pool, Runtime};
use tokio_postgres::{NoTls, Config as PgClientConfig};
use music_evo::web_interface;
use music_evo::user_interaction::{AppState, AUDIO_CACHE_MAX_ENTRIES};
use music_evo::task_queue::{TaskQueue, run_task_queue, run_threshold_checker};
use music_evo::song_queue::{SongQueue, run_song_queue};
use lru::LruCache;
use std::num::NonZeroUsize;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::Mutex;
use std::sync::atomic::AtomicUsize;

#[launch]
async fn rocket() -> _ {
    dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    // Parse the database URL and configure Deadpool.
    let client_config: PgClientConfig = database_url.parse().expect("Invalid DB URL");
    let mut pg_cfg = DpPgConfig::new();
    pg_cfg.host = client_config.get_hosts().get(0).and_then(|host| {
        if let tokio_postgres::config::Host::Tcp(host_str) = host {
            Some(host_str.to_string())
        } else {
            None
        }
    });
    pg_cfg.port = client_config.get_ports().get(0).cloned();
    pg_cfg.user = client_config.get_user().map(|s| s.to_string());
    pg_cfg.password = client_config.get_password().map(|s| String::from_utf8_lossy(s).to_string());
    pg_cfg.dbname = client_config.get_dbname().map(|s| s.to_string());

    let pool: Pool = pg_cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .expect("Error creating pool");

    // Create a broadcast channel for notifications.
    let (notify_tx, _) = broadcast::channel::<()>(10);

    // Create the task queue.
    let (task_queue_sender, task_queue_rx) = TaskQueue::new(100);
    // Create the song queue.
    let (song_queue_sender, song_queue_rx) = tokio::sync::mpsc::channel(100);

    let song_queue_receiver = Arc::new(Mutex::new(song_queue_rx));

    // Check if running in production mode (reproduction handled by external job runner)
    let production_mode = std::env::var("PRODUCTION_MODE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    if production_mode {
        println!("Running in PRODUCTION mode - reproduction handled by external job runner");
    } else {
        println!("Running in DEVELOPMENT mode - reproduction triggered by rating threshold");
    }

    // Build the AppState, embedding the task queue sender.
    // Use LRU-bounded audio cache to limit memory usage
    // Wrap in Arc to allow sharing with task queue worker for cache clearing after reproduction
    let audio_cache = Arc::new(RwLock::new(LruCache::new(
        NonZeroUsize::new(AUDIO_CACHE_MAX_ENTRIES).unwrap()
    )));

    // Check if generation 1 already exists in DB (for server restarts)
    let first_gen_exists = {
        match pool.get().await {
            Ok(client) => {
                match client.query_one(
                    "SELECT COUNT(*) as count FROM songs WHERE generation = 1",
                    &[]
                ).await {
                    Ok(row) => {
                        let count: i64 = row.get("count");
                        if count > 0 {
                            println!("Found {} songs in generation 1, setting first_gen_created=true", count);
                            true
                        } else {
                            false
                        }
                    }
                    Err(e) => {
                        // Table might not exist yet
                        println!("Could not check generation 1 (tables may not exist yet): {}", e);
                        false
                    }
                }
            }
            Err(e) => {
                println!("Could not connect to DB to check generation 1: {}", e);
                false
            }
        }
    };

    let app_state = AppState {
        pool: pool.clone(),
        audio_cache: audio_cache.clone(),
        reproduction_in_progress: Arc::new(AtomicBool::new(false)),
        first_gen_created: Arc::new(AtomicBool::new(first_gen_exists)),
        task_queue: task_queue_sender, // Store the sender for enqueuing tasks.
        song_queue_sender: SongQueue {sender: song_queue_sender, count: Arc::new(AtomicUsize::new(0))}, // Store the sender for song queue.
        song_queue_receiver: song_queue_receiver.clone(), // Store the receiver for processing songs.
        production_mode,
    };

    // Store production_mode for use in fairing closures
    let is_production = production_mode;

    // Build Rocket and attach an on-liftoff fairing to spawn the task queue worker.
    rocket::build()
        .manage(app_state)
        .manage(notify_tx)
        .mount("/", web_interface::routes())
        // Spawn the task queue worker on Rocket's liftoff.
        .attach(AdHoc::on_liftoff("TaskQueue Worker", move |rocket| {
            // Capture the task queue receiver.
            let task_queue_rx = task_queue_rx;
            // Capture audio_cache before moving into async block
            let audio_cache_clone = audio_cache.clone();
            // Get the necessary state for the worker.
            let pool = rocket.state::<AppState>().unwrap().pool.clone();
            let reproduction_flag = rocket.state::<AppState>().unwrap().reproduction_in_progress.clone();
            let notify_tx = rocket.state::<broadcast::Sender<()>>().unwrap().clone();
            let song_queue_receiver = rocket.state::<AppState>().unwrap().song_queue_receiver.clone();

            Box::pin(async move {
                // Spawn a background worker task that runs indefinitely.
                rocket::tokio::spawn(async move {
                    run_task_queue(
                        task_queue_rx, pool, reproduction_flag, notify_tx, song_queue_receiver, audio_cache_clone
                    ).await;
                });
            })
        }))
        // Add threshold checker only in development mode.
        // In production, reproduction is handled by external job runner.
        .attach(AdHoc::on_liftoff("Threshold Checker", move |rocket| {
            let pool = rocket.state::<AppState>().unwrap().pool.clone();
            let reproduction_flag = rocket.state::<AppState>().unwrap().reproduction_in_progress.clone();
            let first_gen_created = rocket.state::<AppState>().unwrap().first_gen_created.clone();
            let task_queue = rocket.state::<AppState>().unwrap().task_queue.clone();
            let is_prod = rocket.state::<AppState>().unwrap().production_mode;

            Box::pin(async move {
                if is_prod {
                    println!("Threshold checker DISABLED in production mode");
                } else {
                    println!("Threshold checker ENABLED in development mode");
                    rocket::tokio::spawn(async move {
                        run_threshold_checker(
                            pool, reproduction_flag, first_gen_created, task_queue
                        ).await;
                    });
                }
            })
        }))
        // Spawn the song queue worker.
        .attach(AdHoc::on_liftoff("SongQueue Worker", move |rocket| {
            let pool = rocket.state::<AppState>().unwrap().pool.clone();
            let reproduction_flag = rocket.state::<AppState>().unwrap().reproduction_in_progress.clone();
            let first_gen_created = rocket.state::<AppState>().unwrap().first_gen_created.clone();
            let song_queue = rocket.state::<AppState>().unwrap().song_queue_sender.clone();
            Box::pin(async move {
                rocket::tokio::spawn(async move {
                    run_song_queue(pool, reproduction_flag, first_gen_created, song_queue, 100).await;
                });
            })
        }))

}
