// src/initialise_experiment.rs
use crate::database::{create_database, populate_habitat_tables};
use crate::genome::Genome;
use crate::genome_crosser::GenomeCrosser;
use postgres::{Client, NoTls};
use std::error::Error;

// import user_interaction
use crate::user_interaction;

pub fn create_adam_and_eve() -> Result<(Genome, Genome), Box<dyn Error>> {
    // let mut adam = Genome::initialise_random_genome(
    //     128,
    //     256,
    //     8,
    //     16,
    // );
    let mut adam= user_interaction::choose_adam()?;
    // set adams mutation rate to 0.03 to give us a good chance of mutation
    adam.assign_mutation_rate(0.03);
    // copy adam to create eve
    let eve = adam.clone_genome();

    Ok((adam, eve))
}

// borrow the client and adam and eve genomes to create generation 1.
pub fn create_generation_1(
    client: &mut Client,
    adam: &Genome,
    eve: &Genome
) -> Result<(), Box<dyn Error>> {
    let generation = 1;

    // Example: retrieve capacity from habitat
    let rows = client.query("SELECT node, capacity FROM habitat", &[])?;
    for row in rows {
        let node_id: i32 = row.get("node");
        let capacity: i32 = row.get("capacity");
        for _ in 0..capacity {
            let child = GenomeCrosser::crossover(adam, eve);

            // Insert child using `query_one` so we can get the new child_id if needed
            let inserted_row = client.query_one(
                "INSERT INTO songs (generation, node, genome, parent1_id, parent2_id)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING song_id",
                &[
                    &generation,
                    &node_id,
                    &child,
                    &(adam.song_id.unwrap_or_default()),
                    &(eve.song_id.unwrap_or_default()),
                ],
            )?;

            let _child_id: i32 = inserted_row.get(0);
            // to get child id:
            // child.assign_song_id(child_id as i32);
        }
    }

    Ok(())
}

pub fn initialise_experiment() -> Result<(), Box<dyn Error>> {
    create_database()?;
    populate_habitat_tables()?;

    let (mut adam, mut eve) = create_adam_and_eve()?;

    let database_url = std::env::var("DATABASE_URL")?;
    let mut client = Client::connect(&database_url, NoTls)?;

    // Insert Adam
    let adam_row = client.query_one(
        "INSERT INTO songs (generation, node, genome)
         VALUES ($1, $2, $3) RETURNING song_id",
        &[&0, &0, &adam],
    )?;
    let adam_id: i32 = adam_row.get(0);
    adam.assign_song_id(adam_id);

    // Insert Eve
    let eve_row = client.query_one(
        "INSERT INTO songs (generation, node, genome)
         VALUES ($1, $2, $3) RETURNING song_id",
        &[&0, &0, &eve],
    )?;
    let eve_id: i32 = eve_row.get(0);
    eve.assign_song_id(eve_id);

    // Create generation 1
    create_generation_1(&mut client, &adam, &eve)?;

    Ok(())
}

pub fn scrub_database() -> Result<(), Box<dyn Error>> {
    // unchanged
    let database_url = std::env::var("DATABASE_URL")?;
    let mut client = Client::connect(&database_url, NoTls)?;
    let mut transaction = client.transaction()?;

    transaction.batch_execute("
        TRUNCATE TABLE dispersal_probabilities, songs, current_generation_fitness,
        historic_fitness_scores, habitat
        RESTART IDENTITY CASCADE;
    ")?;

    transaction.commit()?;
    println!("Database scrubbed and sequences reset.");
    Ok(())
}
