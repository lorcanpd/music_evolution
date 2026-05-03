// src/task_queue.rs
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::sync::RwLock;
use deadpool_postgres::Pool;
use std::error::Error;
use rocket::tokio::sync::broadcast::Sender as BroadcastSender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use lru::LruCache;
use crate::reproduction::differential_reproduction;
use crate::family_trees;

/// A guard that ensures reproduction_in_progress is reset when dropped.
/// This implements the RAII pattern to guarantee cleanup even on panic/error.
struct ReproductionGuard {
    flag: Arc<AtomicBool>,
    notify_tx: BroadcastSender<()>,
}

impl ReproductionGuard {
    fn new(flag: Arc<AtomicBool>, notify_tx: BroadcastSender<()>) -> Self {
        Self { flag, notify_tx }
    }
}

impl Drop for ReproductionGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
        let _ = self.notify_tx.send(());
        eprintln!("ReproductionGuard: reproduction_in_progress set to false");
    }
}

/// The Task enum holds tasks to be processed.
#[derive(Debug)]
pub enum Task {
    Rating { song_id: i32, rating: i32 },
    Reproduction,
}

/// A simple TaskQueue struct that holds the sender.
#[derive(Clone)]
pub struct TaskQueue {
    pub sender: Sender<Task>,
}

impl TaskQueue {
    /// Create a new TaskQueue with a bounded channel.
    pub fn new(bound: usize) -> (Self, Receiver<Task>) {
        let (tx, rx) = channel(bound);
        (Self { sender: tx }, rx)
    }
}

/// Run the task queue worker(s).
pub async fn run_task_queue(
    mut rx: Receiver<Task>,
    pool: Pool,
    reproduction_flag: Arc<AtomicBool>,
    notify_tx: BroadcastSender<()>,
    song_queue_receiver: Arc<Mutex<Receiver<i32>>>,
    audio_cache: Arc<RwLock<LruCache<i32, Arc<Vec<u8>>>>>,
) {
    while let Some(task) = rx.recv().await {
        match task {
            Task::Rating { song_id, rating } => {
                if let Err(e) = process_rating(&pool, song_id, rating).await {
                    eprintln!("Rating task error: {}", e);
                }
            }
            Task::Reproduction => {
                // Drain pending rating tasks – they become irrelevant.
                while let Ok(task) = rx.try_recv() {
                    if let Task::Rating { .. } = task {
                        eprintln!("Dropping pending rating task due to reproduction trigger");
                    }
                }
                // Process reproduction.
                if let Err(e) = process_reproduction(
                    &pool, &notify_tx, &reproduction_flag, &song_queue_receiver, &audio_cache
                ).await {
                    eprintln!("Reproduction task error: {}", e);
                }
            }
        }
    }
}

async fn process_reproduction(
    pool: &Pool,
    notify_tx: &BroadcastSender<()>,
    reproduction_flag: &Arc<AtomicBool>,
    song_queue_receiver: &Arc<Mutex<Receiver<i32>>>,
    audio_cache: &Arc<RwLock<LruCache<i32, Arc<Vec<u8>>>>>,
) -> Result<(), Box<dyn Error>> {
    // Use a guard to ensure reproduction_in_progress is ALWAYS reset,
    // even if we panic or return early with an error.
    // The guard's Drop impl will set the flag to false.
    let _reproduction_guard = ReproductionGuard::new(
        reproduction_flag.clone(),
        notify_tx.clone(),
    );

    let client = pool.get().await?;
    let row = client
        .query_one("SELECT MAX(generation) as curr_gen FROM songs", &[])
        .await?;
    let current_generation: i32 = row.get("curr_gen");
    println!("Task Queue: Current generation: {}", current_generation);
    let next_generation = current_generation + 1;

    // Run differential reproduction - this is the core operation
    differential_reproduction(current_generation, next_generation, pool).await?;

    // Build the previous-generation family-tree package.
    // This is best-effort in development mode and should not block the
    // reproduction lifecycle if the package rebuild fails.
    if let Err(e) = family_trees::update_family_trees_safe(pool, current_generation).await {
        eprintln!("Warning: Failed to update family trees: {}", e);
        eprintln!("Warning: Reproduction will continue despite family tree failure");
    }

    // Clear the audio cache - the old generation's files are no longer valid
    {
        let mut cache = audio_cache.write().await;
        cache.clear();
        println!("Audio cache cleared after reproduction.");
    }

    // NOTE: reproduction_flag.store(false, ...) is handled by _reproduction_guard's Drop
    // We don't need to manually reset it here anymore.

    // Flush the song queue: lock the receiver and drain any pending song IDs.
    {
        let mut rx_lock = song_queue_receiver.lock().await;
        while let Ok(_value) = rx_lock.try_recv() {
            // We don't need the values; just draining.
            eprintln!("Dropping pending song queue item due to reproduction");
        }
        println!("Song queue flushed after reproduction.");
    }

    Ok(())
}

async fn process_rating(pool: &Pool, song_id: i32, rating: i32) -> Result<(), Box<dyn Error>> {
    let client = pool.get().await?;
    client
        .execute(
            "INSERT INTO current_generation_fitness (song_id, rating) VALUES ($1, $2)",
            &[&song_id, &rating],
        )
        .await?;
    Ok(())
}

/// Periodically check the rating threshold and enqueue a reproduction task if needed.
pub async fn run_threshold_checker(
    pool: Pool,
    reproduction_flag: Arc<AtomicBool>,
    first_gen_created: Arc<AtomicBool>,
    task_queue: TaskQueue,
) {
    let mut check_interval = interval(Duration::from_secs(5)); // check every 5 seconds
    loop {
        check_interval.tick().await;
        if !first_gen_created.load(Ordering::SeqCst) {
            continue;
        }
        if reproduction_flag.load(Ordering::SeqCst) {
            continue;
        }
        // Get a DB client to perform the count queries.
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Threshold checker: Failed to get DB connection: {}", e);
                continue;
            }
        };
        // Query total ratings.
        let total_ratings: i64 = match client
            .query_one("SELECT COUNT(*) as count FROM current_generation_fitness", &[])
            .await
        {
            Ok(row) => row.get("count"),
            Err(e) => {
                eprintln!("Threshold checker: Failed to get total ratings: {}", e);
                continue;
            }
        };
        // Query total songs.
        let total_songs: i64 = match client
            .query_one("SELECT COUNT(*) as total FROM songs WHERE generation = (SELECT MAX(generation) FROM songs)", &[])
            .await
        {
            Ok(row) => row.get("total"),
            Err(e) => {
                eprintln!("Threshold checker: Failed to get total songs: {}", e);
                continue;
            }
        };
        // If the threshold is reached, enqueue reproduction.
        if total_ratings >= total_songs * 2 {
            println!("Threshold checker: Rating threshold reached ({} ratings, {} songs)", total_ratings, total_songs);
            reproduction_flag.store(true, Ordering::SeqCst);
            if let Err(e) = task_queue.sender.send(Task::Reproduction).await {
                eprintln!("Threshold checker: Failed to enqueue reproduction task: {}", e);
            }
        }
    }
}
