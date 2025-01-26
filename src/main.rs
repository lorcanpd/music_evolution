// src/main.rs
use dotenv::dotenv;
use std::env;
use postgres::{Client, NoTls};

use crate::initialise_experiment::{initialise_experiment, scrub_database};
use crate::genome::Genome;
use crate::decode_genome::DecodedGenome;
use crate::play_genes::play_genes;
// use crate::user_interaction::rate_songs;

mod initialise_experiment;
mod database;
mod genome;
mod genome_crosser;
mod decode_genome;
mod play_genes;
mod user_interaction;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL")?;
    let mut client = Client::connect(&database_url, NoTls)?;

    initialise_experiment()?;
    println!("Experiment initialised.");

    // For demonstration, let the user rate up to e.g. 8 songs
    println!("For demonstration rate up to 8 songs.");
    user_interaction::rate_songs(8)?;

    // show the fitness scores table. Sum the fitness scores for each song.
    let fitness_scores = client.query(
        "SELECT song_id, SUM(rating) as total_rating \
        FROM current_generation_fitness GROUP BY song_id",
        &[])?;
    for row in fitness_scores.iter() {
        let song_id: i32 = row.get("song_id");
        let rating: i64 = row.get("total_rating");
        println!("Song ID: {}, Rating: {}", song_id, rating);
    }

    scrub_database()?;

    Ok(())
}
