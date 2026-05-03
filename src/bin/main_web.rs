// src/bin/main_web.rs

#[macro_use] extern crate rocket;
extern crate rand;
extern crate maud;
extern crate tokio_postgres;

use dotenv::dotenv;
use rocket::fairing::AdHoc;
use rocket::tokio::sync::broadcast;
use deadpool_postgres::{Config as DpPgConfig, Pool, Runtime};
use tokio_postgres::{NoTls, Config as PgClientConfig};
use music_evo::web_interface;
use music_evo::database::create_database;
use music_evo::user_interaction::{AppState, AUDIO_CACHE_MAX_ENTRIES};
use music_evo::task_queue::{TaskQueue, run_task_queue, run_threshold_checker};
use music_evo::song_queue::{SongQueue, run_song_queue};
use music_evo::family_trees;
use lru::LruCache;
use std::num::NonZeroUsize;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32};
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

    if let Err(error) = create_database(&pool).await {
        eprintln!("Warning: failed to ensure database schema on startup: {}", error);
    }

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

    let current_generation = {
        match pool.get().await {
            Ok(client) => {
                match client.query_one(
                    "SELECT COALESCE(MAX(generation), 1) AS gen FROM songs",
                    &[]
                ).await {
                    Ok(row) => row.get("gen"),
                    Err(e) => {
                        println!("Could not check current generation (tables may not exist yet): {}", e);
                        1
                    }
                }
            }
            Err(e) => {
                println!("Could not connect to DB to check current generation: {}", e);
                1
            }
        }
    };

    let app_state = AppState {
        pool: pool.clone(),
        audio_cache: audio_cache.clone(),
        reproduction_in_progress: Arc::new(AtomicBool::new(false)),
        current_generation: Arc::new(AtomicI32::new(current_generation)),
        first_gen_created: Arc::new(AtomicBool::new(first_gen_exists)),
        task_queue: task_queue_sender, // Store the sender for enqueuing tasks.
        song_queue_sender: SongQueue {sender: song_queue_sender, count: Arc::new(AtomicUsize::new(0))}, // Store the sender for song queue.
        song_queue_receiver: song_queue_receiver.clone(), // Store the receiver for processing songs.
        production_mode,
    };

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
        // Check family trees health on startup and trigger rebuild if needed.
        // This runs in the background and does not block startup.
        .attach(AdHoc::on_liftoff("Family Trees Health Check", move |rocket| {
            let pool = rocket.state::<AppState>().unwrap().pool.clone();
            let first_gen_created = rocket.state::<AppState>().unwrap().first_gen_created.clone();

            Box::pin(async move {
                // Only check if first generation exists (experiment is running)
                if first_gen_created.load(std::sync::atomic::Ordering::SeqCst) {
                    let needs_rebuild = family_trees::needs_rebuild(&pool).await.unwrap_or(true);
                    println!("Family Trees health check: {}", if needs_rebuild { "needs rebuild" } else { "healthy" });

                    if needs_rebuild {
                        // Trigger background rebuild - does not block startup
                        family_trees::ensure_family_trees_background(pool);
                        println!("Family Trees: Background rebuild triggered");
                    }
                } else {
                    println!("Family Trees: Skipping health check (first generation not created yet)");
                }
            })
        }))

}
