use std::collections::HashMap;
use rand::Rng;
use crate::genome_crosser::GenomeCrosser;

#[derive(Clone, Debug)]
pub struct Song {
    pub genome: Vec<u8>,
    pub fitness: f32,
}

#[derive(Debug)]
pub struct Node {
    pub id: usize,
    pub capacity: usize,
    pub songs: Vec<Song>,
}

#[derive(Debug)]
pub struct Edge {
    pub source: usize,
    pub destination: usize,
    pub weight: f32,
}

#[derive(Debug)]
pub struct Graph {
    pub nodes: HashMap<usize, Node>,
    pub edges: Vec<Edge>,
}

impl Graph {
    pub fn new() -> Self {
        Graph {
            nodes: HashMap::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_node(&mut self, id: usize, capacity: usize) {
        self.nodes.insert(id, Node {
            id,
            capacity,
            songs: Vec::new(),
        });
    }

    pub fn add_edge(&mut self, source: usize, destination: usize, weight: f32) {
        self.edges.push(Edge {
            source,
            destination,
            weight,
        });
    }

    pub fn add_song_to_node(&mut self, node_id: usize, song: Song) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            if node.songs.len() < node.capacity {
                node.songs.push(song);
            }
        }
    }

    pub fn calculate_migrations(&self) -> HashMap<usize, Vec<usize>> {
        let mut rng = rand::thread_rng();
        let mut migrations: HashMap<usize, Vec<usize>> = HashMap::new();

        for edge in &self.edges {
            if rng.gen::<f32>() < edge.weight {
                migrations.entry(edge.source).or_insert_with(Vec::new).push(edge.destination);
            }
        }

        migrations
    }

    pub fn reproduce_songs(&mut self, migrations: &HashMap<usize, Vec<usize>>) {
        let mut new_generation: HashMap<usize, Vec<Song>> = HashMap::new();

        // Handle local reproduction
        for (&node_id, node) in &self.nodes {
            let mut rng = rand::thread_rng();
            let total_fitness: f32 = node.songs.iter().map(|s| s.fitness).sum();
            let num_children = node.capacity + migrations.get(&node_id).map_or(0, |v| v.len());

            if node.songs.is_empty() || total_fitness == 0.0 {
                continue;
            }

            for _ in 0..num_children {
                let parent1_index = self.select_parent_index(&node.songs, total_fitness, &mut rng);
                let parent2_index = self.select_parent_index_except(&node.songs, total_fitness, &mut rng, parent1_index);

                let parent1 = &node.songs[parent1_index];
                let parent2 = &node.songs[parent2_index];

                let child_genome = GenomeCrosser::crossover(&parent1.genome, &parent2.genome);
                new_generation.entry(node_id).or_default().push(Song { genome: child_genome, fitness: 0.0 });
            }
        }

        // Collect children to be migrated
        let mut children_to_migrate: Vec<(usize, Song)> = Vec::new();
        for (&source_node, destinations) in migrations {
            if let Some(children) = new_generation.get_mut(&source_node) {
                let mut rng = rand::thread_rng();
                for &destination_node in destinations {
                    if !children.is_empty() {
                        let child_index = rng.gen_range(0..children.len());
                        let child = children.remove(child_index);
                        children_to_migrate.push((destination_node, child));
                    }
                }
            }
        }

        // Transfer children to destination nodes
        for (destination_node, child) in children_to_migrate {
            new_generation.entry(destination_node).or_default().push(child);
        }

        // Update nodes with new generation
        for (node_id, songs) in new_generation {
            if let Some(node) = self.nodes.get_mut(&node_id) {
                node.songs = songs;
            }
        }
    }

    fn select_parent_index(&self, songs: &[Song], total_fitness: f32, rng: &mut rand::prelude::ThreadRng) -> usize {
        let mut cumulative_fitness = 0.0;
        let selection_point = rng.gen_range(0.0..total_fitness);

        for (index, song) in songs.iter().enumerate() {
            cumulative_fitness += song.fitness;
            if cumulative_fitness >= selection_point {
                return index;
            }
        }

        songs.len() - 1
    }

    fn select_parent_index_except(&self, songs: &[Song], total_fitness: f32, rng: &mut rand::prelude::ThreadRng, exclude_index: usize) -> usize {
        let mut cumulative_fitness = 0.0;
        let selection_point = rng.gen_range(0.0..total_fitness - songs[exclude_index].fitness);

        for (index, song) in songs.iter().enumerate() {
            if index == exclude_index {
                continue;
            }
            cumulative_fitness += song.fitness;
            if cumulative_fitness >= selection_point {
                return index;
            }
        }

        if exclude_index == songs.len() - 1 {
            songs.len() - 2
        } else {
            songs.len() - 1
        }
    }

    pub fn calculate_fitness(&mut self, ratings: &HashMap<usize, Vec<f32>>) {
        for (node_id, song_ratings) in ratings {
            if let Some(node) = self.nodes.get_mut(node_id) {
                for (i, rating) in song_ratings.iter().enumerate() {
                    if i < node.songs.len() {
                        node.songs[i].fitness = *rating;
                    }
                }
            }
        }
    }
}
