use postgres::{Client, NoTls, Error, GenericClient};
use std::env;
use dotenv::dotenv;

pub fn create_database() -> Result<(), Error> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let mut client = Client::connect(&database_url, NoTls)?;

    // Create the habitat table
    client.batch_execute("
        CREATE TABLE IF NOT EXISTS habitat (
            node INT NOT NULL,
            capacity INT NOT NULL
        );
    ")?;

    // Create table of dispersal probabilities between nodes.
    client.batch_execute(
        "CREATE TABLE IF NOT EXISTS dispersal_probabilities (
            from_node INT NOT NULL,
            to_node INT NOT NULL,
            probability FLOAT NOT NULL
        );
    ")?;

    // Create the songs table
    client.batch_execute("
        CREATE TABLE IF NOT EXISTS songs (
            generation INT NOT NULL,
            node INT NOT NULL,
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

