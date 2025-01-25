// src/database.rs
use postgres::{Client, NoTls, Error as PgError, GenericClient};
use std::error::Error;

use std::env;
use dotenv::dotenv;

use serde_json;
use std::fs::File;
use std::io::BufReader;
use std::collections::HashMap;



pub fn create_database() -> Result<(), PgError> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let mut client = Client::connect(&database_url, NoTls)?;

    // Create the habitat table. Nodes cannot share the same ID. The capacity is the number of songs
    // that can be stored at each node.
    client.batch_execute(
        "CREATE TABLE IF NOT EXISTS habitat (
            node INT PRIMARY KEY,
            capacity INT NOT NULL
        );
    ")?;

    // Create table of dispersal probabilities between nodes. The from node is a node id from the
    // habitat table. The to node is a node id from the habitat table. The probability is the
    // probability of a song dispersing from the from node to the to node. All to and from node
    // pairs must be unique.
    client.batch_execute(
        "CREATE TABLE IF NOT EXISTS dispersal_probabilities (
            from_node INT NOT NULL REFERENCES habitat(node),
            to_node INT NOT NULL REFERENCES habitat(node),
            probability FLOAT NOT NULL
        );
    ")?;

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
    ")?;

    // Create the current generation fitness table
    client.batch_execute("
        CREATE TABLE IF NOT EXISTS current_generation_fitness (
            song_id INT NOT NULL REFERENCES songs(song_id),
            rating INT NOT NULL,
            timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
    ")?;

    // Create the historic fitness score table
    client.batch_execute("
        CREATE TABLE IF NOT EXISTS historic_fitness_scores (
            song_id INT NOT NULL REFERENCES songs(song_id),
            sum_of_ratings INT NOT NULL
        );
    ")?;

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

pub fn populate_habitat_tables() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let mut client = Client::connect(&database_url, NoTls)?;

    // Read the habitat configuration from a JSON file.
    let file = File::open("habitat_config.json")?;
    let reader = BufReader::new(file);

    let config: HabitatConfig = serde_json::from_reader(reader)?;

    // Insert nodes
    for node in &config.nodes {
        client.execute(
            "INSERT INTO habitat (node, capacity) VALUES ($1, $2)",
            &[&node.id, &node.capacity],
        )?;
    }

    // Insert edges
    for edge in &config.edges {
        client.execute(
            "INSERT INTO dispersal_probabilities (from_node, to_node, probability)\
            VALUES ($1, $2, $3)",
            &[&edge.from_node, &edge.to_node, &edge.probability],
        )?;
    }

    Ok(())
}