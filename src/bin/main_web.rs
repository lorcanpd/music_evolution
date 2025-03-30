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
use music_evo::user_interaction::AppState;
use music_evo::task_queue::{TaskQueue, run_task_queue, run_threshold_checker};
use music_evo::song_queue::{SongQueue, run_song_queue};
use std::collections::HashMap;
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

    // Build the AppState, embedding the task queue sender.
    let app_state = AppState {
        pool: pool.clone(),
        audio_cache: RwLock::new(HashMap::new()),
        reproduction_in_progress: Arc::new(AtomicBool::new(false)),
        first_gen_created: Arc::new(AtomicBool::new(false)),
        task_queue: task_queue_sender, // Store the sender for enqueuing tasks.
        song_queue_sender: SongQueue {sender: song_queue_sender, count: Arc::new(AtomicUsize::new(0))}, // Store the sender for song queue.
        song_queue_receiver: song_queue_receiver.clone(), // Store the receiver for processing songs.
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
            // Get the necessary state for the worker.
            let pool = rocket.state::<AppState>().unwrap().pool.clone();
            let reproduction_flag = rocket.state::<AppState>().unwrap().reproduction_in_progress.clone();
            let notify_tx = rocket.state::<broadcast::Sender<()>>().unwrap().clone();
            let song_queue_receiver = rocket.state::<AppState>().unwrap().song_queue_receiver.clone();

            Box::pin(async move {
                // Spawn a background worker task that runs indefinitely.
                rocket::tokio::spawn(async move {
                    run_task_queue(
                        task_queue_rx, pool, reproduction_flag, notify_tx, song_queue_receiver
                    ).await;
                });
            })
        }))
        // Add this likkle threshold checker to spawn on liftoff.
        .attach(AdHoc::on_liftoff("Threshold Checker", move |rocket| {
            let pool = rocket.state::<AppState>().unwrap().pool.clone();
            let reproduction_flag = rocket.state::<AppState>().unwrap().reproduction_in_progress.clone();
            let first_gen_created = rocket.state::<AppState>().unwrap().first_gen_created.clone();
            // We use the same TaskQueue sender stored in AppState.
            let task_queue = rocket.state::<AppState>().unwrap().task_queue.clone();
            Box::pin(async move {
                rocket::tokio::spawn(async move {
                    run_threshold_checker(
                        pool, reproduction_flag, first_gen_created, task_queue
                    ).await;
                });
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
