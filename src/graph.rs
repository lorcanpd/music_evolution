use std::collections::HashMap;
use rand::Rng;

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
pub struct Graph {
    pub nodes: HashMap<usize, Node>,
}

impl Graph {
    pub fn new() -> Self {
        Graph {
            nodes: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, id: usize, capacity: usize) {
        self.nodes.insert(id, Node {
            id,
            capacity,
            songs: Vec::new(),
        });
    }

    pub fn add_song_to_node(&mut self, node_id: usize, song: Song) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            if node.songs.len() < node.capacity {
                node.songs.push(song);
            }
        }
    }

    pub fn get_random_song(&mut self, node_id: usize) -> Option<Song> {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            if !node.songs.is_empty() {
                let mut rng = rand::thread_rng();
                let index = rng.gen_range(0..node.songs.len());
                return Some(node.songs.remove(index));
            }
        }
        None
    }
}
