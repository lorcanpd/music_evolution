use postgres::{Client, NoTls};
use std::collections::HashMap;
use crate::graph::{Graph, Node, Song};

pub fn load_current_generation(client: &mut Client, generation_number: i32) -> HashMap<usize, Node> {
    let mut nodes = HashMap::new();
    let rows = client.query(
        "SELECT g.id, g.genome, f.score
         FROM genomes g
         LEFT JOIN fitness_scores f ON g.id = f.genome_id
         WHERE g.generation_id = (SELECT id FROM generations WHERE generation_number = $1)",
        &[&generation_number]
    ).unwrap();

    for row in rows {
        let genome: Vec<u8> = row.get("genome");
        let fitness: f32 = row.get("score").unwrap_or(0.0);
        let song = Song { genome, fitness };
        nodes.entry(0).or_insert_with(|| Node { id: 0, capacity: 100, songs: Vec::new() }).songs.push(song);
    }

    nodes
}

pub fn store_fitness_scores(client: &mut Client, fitness_scores: &HashMap<usize, Vec<f32>>) {
    for (genome_id, scores) in fitness_scores {
        for &score in scores {
            client.execute(
                "INSERT INTO fitness_scores (genome_id, score) VALUES ($1, $2)",
                &[&genome_id, &score]
            ).unwrap();
        }
    }
}

pub fn calculate_fitness_and_generate_next_generation(client: &mut Client, current_generation: i32) {
    let next_generation = current_generation + 1;

    // Calculate fitness
    let rows = client.query(
        "SELECT g.id, AVG(f.score) as avg_score
         FROM genomes g
         JOIN fitness_scores f ON g.id = f.genome_id
         WHERE g.generation_id = (SELECT id FROM generations WHERE generation_number = $1)
         GROUP BY g.id",
        &[&current_generation]
    ).unwrap();

    let mut fitness_scores = HashMap::new();
    for row in rows {
        let genome_id: i32 = row.get("id");
        let avg_score: f32 = row.get("avg_score");
        fitness_scores.insert(genome_id as usize, avg_score);
    }

    // Generate next generation
    let mut graph = Graph::new();
    graph.calculate_fitness(&fitness_scores);
    let migrations = graph.calculate_migrations();
    graph.reproduce_songs(&migrations);

    // Store next generation
    client.execute(
        "INSERT INTO generations (generation_number) VALUES ($1)",
        &[&next_generation]
    ).unwrap();
    let generation_id: i32 = client.query_one(
        "SELECT id FROM generations WHERE generation_number = $1",
        &[&next_generation]
    ).unwrap().get("id");

    for node in graph.nodes.values() {
        for song in &node.songs {
            client.execute(
                "INSERT INTO genomes (generation_id, genome, parent1_id, parent2_id) VALUES ($1, $2, NULL, NULL)",
                &[&generation_id, &song.genome]
            ).unwrap();
        }
    }
}