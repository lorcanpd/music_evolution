// src/bin/init_experiment.rs

use music_evo::initialise_experiment::initialise_experiment;

#[tokio::main]
async fn main() {
    if let Err(e) = initialise_experiment().await {
        eprintln!("Error: {}", e);
    }
}
