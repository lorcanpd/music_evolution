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
mod reproduction;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL")?;
    let mut client = Client::connect(&database_url, NoTls)?;

    initialise_experiment()?;
    println!("Experiment initialised.");

    // get total number of songs possible using the habitat table. Use get to get the value from the
    // row in one go.
    let total_songs = client.query_one(
        "SELECT SUM(capacity) as total_songs FROM habitat",
        &[])?;
    // let total_songs: i64 = total_songs.get("total_songs");

    let total_songs = 8;

    loop {
        println!("We will sample and rate {} songs from the world.", total_songs);
        user_interaction::rate_songs(total_songs as i32)?;

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

        // get current generation
        let mut current_generation = client.query_one(
            "SELECT MAX(generation) as curr_gen FROM songs",
            &[])?.get::<_, i32>("curr_gen");

        // produce the next generation
        reproduction::differential_reproduction(
            current_generation, current_generation + 1)?;

        println!("Type 'q' to quit or Enter to continue.");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim().eq_ignore_ascii_case("q") {
            break;
        }
    }

    scrub_database()?;

    Ok(())
}
