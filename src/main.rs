// src/main.rs
use dotenv::dotenv;
use std::env;
use postgres::{Client, NoTls};
use crate::initialise_experiment::{initialise_experiment, scrub_database};
use crate::genome::Genome;
use crate::decode_genome::DecodedGenome;
use crate::play_genes::play_genes;

mod initialise_experiment;
mod database;
mod genome;
mod genome_crosser;
mod decode_genome;
mod play_genes;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env file
    dotenv().ok();

    // Retrieve the DATABASE_URL environment variable
    let database_url = env::var("DATABASE_URL")?;

    // Attempt to connect to the PostgreSQL database
    let mut client = Client::connect(&database_url, NoTls)?;

    // Check the connection by executing a simple query
    client.batch_execute("SELECT 1")?;

    println!("Successfully connected to the database and executed query.");

    // Initialise the experiment
    initialise_experiment()?;

    println!("Experiment initialised.");

    // Now let's play Adam and Eve and then all their children from generation 1.
    // First, get the songs from the database.
    // Then, decode the genome.
    // Then, play the genome.

    // Playing Adam (song_id = 1)
    println!("Playing Adam");
    let adam_request = client.query("SELECT genome FROM songs WHERE song_id = 1", &[])?;
    for row in adam_request.iter() {
        let genome: Genome = row.get("genome");
        let decoded: DecodedGenome = DecodedGenome::decode(&genome);
        play_genes(&decoded)?;
    }

    // Playing Eve (song_id = 2)
    println!("Playing Eve");
    let eve_request = client.query("SELECT genome FROM songs WHERE song_id = 2", &[])?;
    for row in eve_request.iter() {
        let genome: Genome = row.get("genome");
        let decoded: DecodedGenome = DecodedGenome::decode(&genome);
        play_genes(&decoded)?;
    }

    // Playing children (generation = 1)
    println!("Playing children");
    let children_request = client.query("SELECT genome FROM songs WHERE generation = 1", &[])?;
    for row in children_request.iter() {
        let genome: Genome = row.get("genome");
        let decoded: DecodedGenome = DecodedGenome::decode(&genome);
        play_genes(&decoded)?;
    }

    println!("Test complete.");

    // Scrub the database
    scrub_database()?;

    println!("Database scrubbed.");

    Ok(())
}
