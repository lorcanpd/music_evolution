use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FamilyTreesStatus {
    pub status: String,
    pub last_updated: Option<String>,
    pub last_error: Option<String>,
    pub previous_generation: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IslandPalette {
    pub node: i32,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopulationSong {
    pub song_id: i32,
    pub generation: i32,
    pub node: i32,
    pub sum_of_ratings: i32,
    pub is_spotlight: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotlightSummary {
    pub index: usize,
    pub song_id: i32,
    pub node: i32,
    pub sum_of_ratings: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FamilyTreesMetadata {
    pub previous_generation: i32,
    pub built_at: String,
    pub islands: Vec<IslandPalette>,
    pub population: Vec<PopulationSong>,
    pub spotlights: Vec<SpotlightSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub song_id: i32,
    pub generation: i32,
    pub node: i32,
    pub role: String,
    pub spotlight_similarity: f32,
    pub x: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEdge {
    pub parent_song_id: i32,
    pub child_song_id: i32,
    pub similarity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevealPath {
    pub target_song_id: i32,
    pub node_path: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotlightTree {
    pub spotlight_song_id: i32,
    pub previous_generation: i32,
    pub nodes: Vec<TreeNode>,
    pub edges: Vec<TreeEdge>,
    pub reveal_paths: Vec<RevealPath>,
}
