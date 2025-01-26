// src/bin/scrub_db.rs
use music_evo::initialise_experiment::scrub_database;

#[tokio::main]
async fn main() {
    if let Err(e) = scrub_database().await {
        eprintln!("Error: {}", e);
    }
}
