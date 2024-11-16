mod genome;
mod decode_genome;
mod play_genes;
mod genome_crosser;
mod graph;
mod database;

use genome::Genome;
use decode_genome::DecodedGenome;
use play_genes::play_genes;
use genome_crosser::GenomeCrosser;
use postgres::{Client, NoTls};
use rand::Rng;
use std::collections::HashMap;

fn main() {
    let mut client = Client::connect("host=localhost user=postgres", NoTls).unwrap();

    // Load current generation
    let current_generation = 1;
    let nodes = database::load_current_generation(&mut client, current_generation);

    // Simulate rating process
    let mut fitness_scores: HashMap<usize, Vec<f32>> = HashMap::new();
    for (node_id, node) in &nodes {
        let scores: Vec<f32> = node.songs.iter().map(|_| rand::thread_rng().gen_range(0.0..1.0)).collect();
        fitness_scores.insert(*node_id, scores);
    }

    // Store fitness scores
    database::store_fitness_scores(&mut client, &fitness_scores);

    // Calculate fitness and generate next generation
    database::calculate_fitness_and_generate_next_generation(&mut client, current_generation);
}