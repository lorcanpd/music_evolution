// src/song_queue.rs

use tokio::sync::mpsc::{Receiver, Sender, channel};
use deadpool_postgres::Pool;
use tokio::time::{interval, Duration};
use std::error::Error;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// A simple SongQueue that holds pre-fetched song IDs.
#[derive(Clone)]
pub struct SongQueue {
    pub sender: Sender<i32>,
    pub count: Arc<AtomicUsize>,
}

impl SongQueue {
    /// Create a new SongQueue with a bounded channel.
    pub fn new(bound: usize) -> (Self, Receiver<i32>) {
        let (tx, rx) = channel(bound);
        let queue = SongQueue {
            sender: tx,
            count: Arc::new(AtomicUsize::new(0)),
        };
        (queue, rx)
    }
    /// Try to send a song ID. On success, increment the count.
    pub async fn try_send_song(&self, song_id: i32) -> Result<(), ()> {
        match self.sender.try_send(song_id) {
            Ok(()) => {
                self.count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(_) => Err(()),
        }
    }

    /// Called when a song is consumed.
    pub fn song_consumed(&self) {
        self.count.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Periodically checks the song queue. If there are fewer than the threshold songs and reproduction is not in progress,
/// then it queries the database for a random song (from the current generation) and tries to send it on the channel.
pub async fn run_song_queue(
    pool: Pool,
    reproduction_flag: Arc<AtomicBool>,
    first_gen_created: Arc<AtomicBool>,
    song_queue: SongQueue,
    threshold: usize,
) {
    let mut check_interval = interval(Duration::from_secs(5));
    loop {
        check_interval.tick().await;
        if !first_gen_created.load(Ordering::SeqCst) {
            continue;
        }
        // Skip if reproduction is in progress.
        if reproduction_flag.load(Ordering::SeqCst) {
            continue;
        }

        // Check the current count.
        let current = song_queue.count.load(Ordering::Relaxed);
        if current < threshold {
            // We want to top up the queue until it has `threshold` items.
            if let Ok(client) = pool.get().await {
                if let Ok(row) = client
                    .query_one(
                        "SELECT song_id FROM songs \
                        WHERE generation = (SELECT MAX(generation) FROM songs) \
                        ORDER BY RANDOM() LIMIT 1",
                        &[],
                    )
                    .await
                {
                    let song_id: i32 = row.get("song_id");
                    // Await the send operation.
                    let _ = song_queue.try_send_song(song_id).await;
                }
            }
        }
    }
}
