// src/initialise_experiment.rs

use deadpool_postgres::Pool;
use crate::database::{create_database, populate_habitat_tables};
use crate::genome::Genome;
use crate::genome_crosser::GenomeCrosser;
use crate::decode_genome::DecodedGenome;
use crate::play_genes;
use crate::audio_files;
use std::error::Error;
use std::path::Path;
use crate::user_interaction;

pub async fn create_adam_and_eve() -> Result<(Genome, Genome), Box<dyn Error>> {
    let mut adam = user_interaction::choose_adam()?;
    adam.assign_mutation_rate(0.03);
    let eve = adam.clone_genome();
    Ok((adam, eve))
}

pub async fn store_current_generation_wavs(pool: &Pool) -> Result<(), Box<dyn Error>> {
    // Initialize audio directory structure and create generation 1 directory
    audio_files::init_audio_dirs()?;
    let gen_dir = audio_files::create_generation_dir(1)?;

    let client = pool.get().await?;
    let rows = client.query("SELECT song_id, genome FROM songs WHERE generation=1", &[]).await?;
    for row in rows {
        let song_id: i32 = row.get("song_id");
        let genome: Genome = row.get("genome");
        let decoded = DecodedGenome::decode(&genome);
        let filename = gen_dir.join(format!("{}.wav", song_id));
        play_genes::generate_wav(&decoded, filename.to_str().unwrap())?;
        println!("Created WAV file for song_id={} at {}", song_id, filename.display());
    }

    // Activate generation 1 (create the symlink)
    audio_files::activate_generation(1)?;

    Ok(())
}

pub async fn create_generation_1(
    pool: &Pool,
    adam: &Genome,
    eve: &Genome
) -> Result<(), Box<dyn Error>> {
    let generation = 1;
    let client = pool.get().await?;
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
                &[&generation, &node_id, &child, &1, &2],
            ).await?;
            let _child_id: i32 = inserted_row.get(0);
        }
    }
    Ok(())
}

pub async fn initialise_experiment(pool: &Pool) -> Result<(), Box<dyn Error>> {
    create_database(pool).await?;
    populate_habitat_tables(pool).await?;

    let (mut adam, mut eve) = create_adam_and_eve().await?;
    let client = pool.get().await?;

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

    create_generation_1(pool, &adam, &eve).await?;
    store_current_generation_wavs(pool).await?;

    Ok(())
}

pub async fn scrub_database(pool: &Pool) -> Result<(), Box<dyn Error>> {
    let mut client = pool.get().await?;
    let transaction = client.transaction().await?;
    transaction.batch_execute("
        TRUNCATE TABLE dispersal_probabilities, songs, current_generation_fitness,
        historic_fitness_scores, habitat
        RESTART IDENTITY CASCADE;
    ").await?;
    transaction.commit().await?;
    println!("Database scrubbed and sequences reset.");

    // Clean up audio directories (handles symlinks properly)
    audio_files::scrub_audio_dirs()?;
    println!("Audio directories scrubbed.");

    // Also remove legacy current_generation if it exists
    if Path::new("current_generation").exists() {
        let _ = std::fs::remove_dir_all("current_generation");
    }

    Ok(())
}
