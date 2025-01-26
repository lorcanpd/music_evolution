// src/initialise_experiment.rs
use crate::database::{create_database, populate_habitat_tables};
use crate::genome::Genome;
use crate::genome_crosser::GenomeCrosser;
use crate::decode_genome::DecodedGenome;
use crate::play_genes;
use tokio_postgres::Client as AsyncClient;
use tokio_postgres::NoTls;
use tokio_postgres::connect;
use std::error::Error;
use std::fs;
use std::path::Path;
use dotenv::dotenv;
use rocket::response::Redirect;
// import user_interaction
use crate::user_interaction;


pub async fn create_adam_and_eve() -> Result<(Genome, Genome), Box<dyn Error>> {
    let mut adam = user_interaction::choose_adam()?;
    adam.assign_mutation_rate(0.03);
    let eve = adam.clone_genome();

    Ok((adam, eve))
}

pub async fn store_current_generation_wavs(client: &AsyncClient) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all("current_generation")?;

    let rows = client.query("SELECT song_id, genome FROM songs WHERE generation=1", &[]).await?;

    for row in rows {
        let song_id: i32 = row.get("song_id");
        let genome: Genome = row.get("genome");

        let decoded = DecodedGenome::decode(&genome);
        let filename = format!("current_generation/{}.wav", song_id);

        play_genes::generate_wav(&decoded, &filename)?;
        println!("Created WAV file for song_id={} at {}", song_id, filename);
    }

    Ok(())
}

pub async fn create_generation_1(
    client: &AsyncClient,
    adam: &Genome,
    eve: &Genome
) -> Result<(), Box<dyn Error>> {
    let generation = 1;

    let rows = client.query("SELECT node, capacity FROM habitat", &[]).await?;
    for row in rows {
        let node_id: i32 = row.get("node");
        let capacity: i32 = row.get("capacity");
        for _ in 0..capacity {
            let child = GenomeCrosser::crossover(adam, eve);

            let inserted_row = client.query_one(
                "INSERT INTO songs (generation, node, genome, parent1_id, parent2_id)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING song_id",
                &[
                    &generation,
                    &node_id,
                    &child,
                    &1,
                    &2,
                ],
            ).await?;

            let _child_id: i32 = inserted_row.get(0);
        }
    }

    Ok(())
}

pub async fn initialise_experiment() -> Result<(), Box<dyn Error>> {
    create_database().await?;
    populate_habitat_tables().await?;

    let (mut adam, mut eve) = create_adam_and_eve().await?;

    let database_url = std::env::var("DATABASE_URL")?;
    let (client, connection) = connect(&database_url, NoTls).await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    let adam_row = client.query_one(
        "INSERT INTO songs (generation, node, genome)
        VALUES ($1, $2, $3) RETURNING song_id",
        &[&0, &0, &adam],
    ).await?;
    let adam_id: i32 = adam_row.get(0);
    adam.assign_song_id(adam_id);

    let eve_row = client.query_one(
        "INSERT INTO songs (generation, node, genome)
        VALUES ($1, $2, $3) RETURNING song_id",
        &[&0, &0, &eve],
    ).await?;
    let eve_id: i32 = eve_row.get(0);
    eve.assign_song_id(eve_id);

    create_generation_1(&client, &adam, &eve).await?;
    store_current_generation_wavs(&client).await?;

    Ok(())
}

pub async fn scrub_database() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    let (mut client, connection) = connect(&database_url, NoTls).await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    let transaction = client.transaction().await?;
    transaction.batch_execute("
        TRUNCATE TABLE dispersal_probabilities, songs, current_generation_fitness,
        historic_fitness_scores, habitat
        RESTART IDENTITY CASCADE;
    ").await?;

    transaction.commit().await?;
    println!("Database scrubbed and sequences reset.");

    if Path::new("current_generation").exists() {
        fs::remove_dir_all("current_generation")?;
        println!("Removed current_generation folder.");
    }

    Ok(())
}
