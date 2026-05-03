// src/database.rs

use deadpool_postgres::Pool;
use std::error::Error;
use serde_json;
use std::fs::File;
use std::io::BufReader;

// pub async fn create_database() -> Result<(), PgError> {
pub async fn create_database(pool: &Pool) -> Result<(), Box<dyn Error>> {

    let client = pool.get().await?;

    // Create the habitat table. Nodes cannot share the same ID. The capacity is the number of songs
    // that can be stored at each node.
    client.batch_execute(
        "CREATE TABLE IF NOT EXISTS habitat (
            node INT PRIMARY KEY,
            capacity INT NOT NULL
        );
    ").await?;

    // Create table of dispersal probabilities between nodes. The 'from node' is a node id from the
    // habitat table. The 'to node' is a node id from the habitat table. The probability is the
    // probability of a song dispersing from the from node to the to node. All to and from node
    // pairs must be unique.
    client.batch_execute(
        "CREATE TABLE IF NOT EXISTS dispersal_probabilities (
            from_node INT NOT NULL REFERENCES habitat(node),
            to_node INT NOT NULL REFERENCES habitat(node),
            probability FLOAT NOT NULL,
            UNIQUE (from_node, to_node)
        );
    ").await?;

    // Add unique constraint if table already exists without it (migration for existing DBs)
    let _ = client.batch_execute(
        "ALTER TABLE dispersal_probabilities
         ADD CONSTRAINT dispersal_probabilities_unique UNIQUE (from_node, to_node);"
    ).await; // Ignore error if constraint already exists

    // Create the songs table
    client.batch_execute("
        CREATE TABLE IF NOT EXISTS songs (
            generation INT NOT NULL,
            node INT NOT NULL REFERENCES habitat(node),
            song_id SERIAL PRIMARY KEY,
            parent1_id INT REFERENCES songs(song_id),
            parent2_id INT REFERENCES songs(song_id),
            genome BYTEA NOT NULL
        );
    ").await?;

    // Create the current generation fitness table
    client.batch_execute("
        CREATE TABLE IF NOT EXISTS current_generation_fitness (
            song_id INT NOT NULL REFERENCES songs(song_id),
            rating INT NOT NULL,
            timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
    ").await?;

    // Create the historic fitness score table
    client.batch_execute("
        CREATE TABLE IF NOT EXISTS historic_fitness_scores (
            song_id INT NOT NULL REFERENCES songs(song_id),
            sum_of_ratings INT NOT NULL
        );
    ").await?;

    // Store finalized support values for previous generations so the
    // family-tree package can be rebuilt without preserving full archives.
    client.batch_execute("
        CREATE TABLE IF NOT EXISTS previous_generation_fitness (
            generation INT NOT NULL,
            song_id INT NOT NULL REFERENCES songs(song_id),
            node INT NOT NULL REFERENCES habitat(node),
            sum_of_ratings INT NOT NULL,
            PRIMARY KEY (generation, song_id)
        );
    ").await?;

    Ok(())
}

#[derive(serde::Deserialize)]
struct HabitatConfig {
    nodes: Vec<HabitatNode>,
    edges: Vec<HabitatEdge>,
}

#[derive(serde::Deserialize)]
struct HabitatNode {
    id: i32,
    capacity: i32,
}

#[derive(serde::Deserialize)]
struct HabitatEdge {
    from_node: i32,
    to_node: i32,
    probability: f64,
}

pub async fn populate_habitat_tables(pool: &Pool) -> Result<(), Box<dyn Error>> {
    let client = pool.get().await?;

    // Check if habitat is already populated
    let count: i64 = client
        .query_one("SELECT COUNT(*) as count FROM habitat", &[])
        .await?
        .get("count");

    if count > 0 {
        println!("Habitat tables already populated ({} nodes), skipping.", count);
        return Ok(());
    }

    // Read the habitat configuration from a JSON file.
    let file = File::open("habitat_config.json")?;
    let reader = BufReader::new(file);

    let config: HabitatConfig = serde_json::from_reader(reader)?;

    // Insert nodes
    for node in &config.nodes {
        client.execute(
            "INSERT INTO habitat (node, capacity) VALUES ($1, $2) ON CONFLICT (node) DO NOTHING",
            &[&node.id, &node.capacity],
        ).await?;
    }

    // Insert edges
    for edge in &config.edges {
        client.execute(
            "INSERT INTO dispersal_probabilities (from_node, to_node, probability) VALUES ($1, $2, $3)
             ON CONFLICT DO NOTHING",
            &[&edge.from_node, &edge.to_node, &(edge.probability as f64)],
        ).await?;
    }

    println!("Habitat tables populated with {} nodes.", config.nodes.len());
    Ok(())
}
