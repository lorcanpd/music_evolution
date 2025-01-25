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

    // Quick test of DB connectivity
    client.batch_execute("SELECT 1")?;
    println!("Successfully connected to the database and executed query.");

    // 1. Initialise the experiment (user picks Adam, node population, etc.)
    initialise_experiment()?;
    println!("Experiment initialised.");

    // 2. Play Adam
    // println!("Playing Adam");
    // let adam_request = client.query("SELECT genome FROM songs WHERE song_id = 1", &[])?;
    // for row in adam_request.iter() {
    //     let genome: Genome = row.get("genome");
    //     let decoded = DecodedGenome::decode(&genome);
    //     play_genes(&decoded)?;
    // }
    //
    // // 3. Play Eve
    // println!("Playing Eve");
    // let eve_request = client.query("SELECT genome FROM songs WHERE song_id = 2", &[])?;
    // for row in eve_request.iter() {
    //     let genome: Genome = row.get("genome");
    //     let decoded = DecodedGenome::decode(&genome);
    //     play_genes(&decoded)?;
    // }

    // 4. Play generation 1
    // println!("Playing Adam and Eve's children.");
    // let children_request = client.query("SELECT genome FROM songs WHERE generation = 1", &[])?;
    // for row in children_request.iter() {
    //     let genome: Genome = row.get("genome");
    //     let decoded = DecodedGenome::decode(&genome);
    //     play_genes(&decoded)?;
    // }
    //
    // println!("Test complete.");

    // 5. Rate Songs
    // For demonstration, let the user rate up to e.g. 8 songs
    println!("For demonstration rate up to 8 songs.");
    user_interaction::rate_songs(8)?;

    // show the fitness scores table
    let fitness_scores = client.query("SELECT * FROM current_generation_fitness", &[])?;
    for row in fitness_scores.iter() {
        let song_id: i32 = row.get("song_id");
        let rating: i32 = row.get("rating");
        println!("Song ID: {}, Rating: {}", song_id, rating);
    }

    scrub_database()?;

    Ok(())
}
