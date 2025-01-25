// src/user_interaction.rs
use std::error::Error;
use std::io::{self, Write};
use rand::Rng;
use postgres::{Client, NoTls};

use crate::genome::Genome;
use crate::decode_genome::DecodedGenome;
use crate::play_genes::play_genes;

/// Prompt the user to accept or reject a randomly generated Adam genome.
/// Returns the accepted Adam genome.
pub fn choose_adam() -> Result<Genome, Box<dyn Error>> {
    loop {
        // Generate a random Adam genome
        let mut adam = Genome::initialise_random_genome(128, 256, 8, 16);
        adam.assign_mutation_rate(0.02);

        println!("Generated a new Adam with random mutation rate. Accept this Adam? (y/n): ");
        // play adam
        let decoded = DecodedGenome::decode(&adam);
        play_genes(&decoded)?;

        // Prompt user
        print!("> ");
        io::stdout().flush()?; // ensure prompt is displayed

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input == "y" || input == "yes" {
            println!("Adam accepted.");
            return Ok(adam);
        } else if input == "n" || input == "no" {
            println!("Generating a new Adam...");
            // loop continues, generating a new one
        } else {
            println!("Please type 'y'/'yes' to approve or 'n'/'no' to reject.");
        }
    }
}

/// Randomly sample and rate songs until a given rating limit is reached.
/// For each rating, store it in the `current_generation_fitness` table.
///
/// - `rating_limit`: number of total ratings to collect before stopping.
/// - After collecting `rating_limit` ratings, scrub the database (for now).
pub fn rate_songs(rating_limit: usize) -> Result<(), Box<dyn Error>> {
    // Retrieve the DATABASE_URL environment variable
    let database_url = std::env::var("DATABASE_URL")?;
    let mut client = Client::connect(&database_url, NoTls)?;

    let mut rng = rand::thread_rng();
    let mut ratings_collected = 0;

    println!("Starting rating process. Type 1..5 for your rating, or 'q' to quit early.");

    while ratings_collected < rating_limit {
        // // 1. Randomly pick a song from the 'songs' table
        // //    Count total songs first:
        // let row_count: i64 = client.query_one("SELECT COUNT(*) FROM songs where generate = max(generation)", &[])?.get(0);
        // if row_count == 0 {
        //     println!("No songs in the database. Exiting...");
        //     break;
        // }
        // get the song_ids of the current generation of songs
        let song_ids_rows = client.query("SELECT song_id FROM songs WHERE generation = (SELECT MAX(generation) FROM songs)", &[])?;

        // get song_ids from the rows
        let song_ids: Vec<i32> = song_ids_rows.iter().map(|row| row.get("song_id")).collect();
        let row_count = song_ids.len() as i64;

        // if for some reason no row returned, continue
        if song_ids.is_empty() {
            println!("No songs in the database. Exiting...");
            break;
        }



        // // pick a random offset
        // let offset = rng.gen_range(0..row_count) as i64;
        // let rows = client.query(
        //     "SELECT song_id, genome FROM songs OFFSET $1 LIMIT 1",
        //     &[&offset],
        // )?;

        // pick a random song_id
        let song_id = song_ids[rng.gen_range(0..song_ids.len())];
        let rows = client.query(
            "SELECT song_id, genome FROM songs WHERE song_id = $1",
            &[&song_id],
        )?;

        // if for some reason no row returned, continue
        if rows.is_empty() {
            continue;
        }

        // play the song
        println!("Playing song_id={}", song_id);
        for row in rows.iter() {
            let genome: Genome = row.get("genome");
            let decoded: DecodedGenome = DecodedGenome::decode(&genome);
            play_genes(&decoded)?;
        }

        // 2. Prompt the user for a rating
        println!("Please rate song_id={} with a value between 1..5, or 'q' to quit:", song_id);

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("q") {
            println!("Quitting rating early...");
            break;
        }

        // parse as i32
        let rating: i32 = match input.parse() {
            Ok(val @ 1..=5) => val,
            _ => {
                println!("Invalid rating. Please type a number between 1..5 or 'q'.");
                continue;
            }
        };

        // 3. Store rating in current_generation_fitness
        client.execute(
            "INSERT INTO current_generation_fitness (song_id, rating)
             VALUES ($1, $2)",
            &[&song_id, &rating],
        )?;

        ratings_collected += 1;
        println!("Recorded rating for song {}, total ratings = {}", song_id, ratings_collected);
    }

    Ok(())
}
