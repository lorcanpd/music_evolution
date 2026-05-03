use std::collections::{BTreeSet, HashMap, VecDeque};
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use deadpool_postgres::Pool;
use once_cell::sync::Lazy;
use serde_json;
use tokio::sync::Mutex;

use crate::decode_genome::DecodedGenome;
use crate::family_tree_types::{
    FamilyTreesMetadata, FamilyTreesStatus, IslandPalette, PopulationSong, RevealPath,
    SpotlightSummary, SpotlightTree, TreeEdge, TreeNode,
};
use crate::genome::Genome;
use crate::play_genes;
use crate::relatedness;

static REBUILD_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static REBUILD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

pub const DATA_BASE_DIR: &str = "data";
pub const FAMILY_TREES_SUBDIR: &str = "family_trees";
pub const SLOTS_SUBDIR: &str = "slots";
pub const CURRENT_SYMLINK_NAME: &str = "current";
pub const STATUS_FILENAME: &str = "status.json";
pub const METADATA_FILENAME: &str = "metadata.json";
pub const SPOTS_SUBDIR: &str = "spots";
pub const AUDIO_SUBDIR: &str = "audio";

const SLOT_A: &str = "a";
const SLOT_B: &str = "b";

#[derive(Clone)]
struct SongRecord {
    song_id: i32,
    generation: i32,
    node: i32,
    parent1_id: Option<i32>,
    parent2_id: Option<i32>,
    genome: Genome,
}

#[derive(Clone)]
struct PopulationRecord {
    song_id: i32,
    generation: i32,
    node: i32,
    sum_of_ratings: i32,
}

pub fn family_trees_base_path() -> PathBuf {
    PathBuf::from(DATA_BASE_DIR).join(FAMILY_TREES_SUBDIR)
}

pub fn slots_path() -> PathBuf {
    family_trees_base_path().join(SLOTS_SUBDIR)
}

pub fn slot_path(name: &str) -> PathBuf {
    slots_path().join(name)
}

pub fn current_symlink_path() -> PathBuf {
    family_trees_base_path().join(CURRENT_SYMLINK_NAME)
}

pub fn serving_metadata_path() -> PathBuf {
    current_symlink_path().join(METADATA_FILENAME)
}

pub fn serving_spot_path(index: usize) -> PathBuf {
    current_symlink_path().join(SPOTS_SUBDIR).join(format!("{}.json", index))
}

pub fn serving_audio_dir() -> PathBuf {
    current_symlink_path().join(AUDIO_SUBDIR)
}

pub fn wav_file_path(song_id: i32) -> PathBuf {
    serving_audio_dir().join(format!("{}.wav", song_id))
}

pub fn status_file_path() -> PathBuf {
    family_trees_base_path().join(STATUS_FILENAME)
}

pub fn save_status(status: &FamilyTreesStatus) -> io::Result<()> {
    let json = serde_json::to_string_pretty(status)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(status_file_path(), json)
}

pub fn load_status() -> Option<FamilyTreesStatus> {
    fs::read_to_string(status_file_path())
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
}

pub fn load_current_metadata() -> Result<FamilyTreesMetadata, Box<dyn Error>> {
    let json = fs::read_to_string(serving_metadata_path())?;
    Ok(serde_json::from_str(&json)?)
}

pub fn load_spotlight_tree(index: usize) -> Result<SpotlightTree, Box<dyn Error>> {
    let json = fs::read_to_string(serving_spot_path(index))?;
    Ok(serde_json::from_str(&json)?)
}

pub fn is_rebuild_in_progress() -> bool {
    REBUILD_IN_PROGRESS.load(Ordering::SeqCst)
}

pub fn init_dirs() -> io::Result<()> {
    fs::create_dir_all(slot_path(SLOT_A))?;
    fs::create_dir_all(slot_path(SLOT_B))?;
    Ok(())
}

fn current_slot_name() -> Option<String> {
    let symlink = current_symlink_path();
    if !symlink.exists() {
        return None;
    }

    fs::read_link(symlink)
        .ok()
        .and_then(|target| target.file_name().and_then(|name| name.to_str()).map(|s| s.to_string()))
}

fn inactive_slot_name() -> &'static str {
    match current_slot_name().as_deref() {
        Some(SLOT_A) => SLOT_B,
        _ => SLOT_A,
    }
}

fn prepare_slot(name: &str) -> io::Result<PathBuf> {
    let base = slot_path(name);
    if base.exists() {
        fs::remove_dir_all(&base)?;
    }
    fs::create_dir_all(base.join(SPOTS_SUBDIR))?;
    fs::create_dir_all(base.join(AUDIO_SUBDIR))?;
    Ok(base)
}

fn activate_slot(name: &str) -> Result<(), Box<dyn Error>> {
    let symlink = current_symlink_path();
    let temp_symlink = family_trees_base_path().join(".current_new");
    let relative_target = PathBuf::from(SLOTS_SUBDIR).join(name);

    let _ = fs::remove_file(&temp_symlink);

    #[cfg(unix)]
    std::os::unix::fs::symlink(&relative_target, &temp_symlink)?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&relative_target, &temp_symlink)?;

    fs::rename(&temp_symlink, &symlink)?;
    Ok(())
}

pub fn is_healthy() -> bool {
    let current = current_symlink_path();
    if !current.exists() {
        return false;
    }

    let metadata = current.join(METADATA_FILENAME);
    let spots = current.join(SPOTS_SUBDIR);
    let audio = current.join(AUDIO_SUBDIR);
    if !metadata.exists() || !spots.is_dir() || !audio.is_dir() {
        return false;
    }

    let metadata_json = match fs::read_to_string(metadata) {
        Ok(json) => json,
        Err(_) => return false,
    };

    let metadata: FamilyTreesMetadata = match serde_json::from_str(&metadata_json) {
        Ok(parsed) => parsed,
        Err(_) => return false,
    };

    for spot in &metadata.spotlights {
        if !spots.join(format!("{}.json", spot.index)).exists() {
            return false;
        }
    }

    true
}

pub async fn latest_generation(pool: &Pool) -> Result<i32, Box<dyn Error + Send + Sync>> {
    let client = pool.get().await?;
    let generation: i32 = client
        .query_one("SELECT COALESCE(MAX(generation), 0) AS gen FROM songs", &[])
        .await?
        .get("gen");
    Ok(generation)
}

pub async fn needs_rebuild(pool: &Pool) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let latest = latest_generation(pool).await?;
    if latest < 2 {
        return Ok(false);
    }

    if !is_healthy() {
        return Ok(true);
    }

    let metadata = match load_current_metadata() {
        Ok(metadata) => metadata,
        Err(_) => return Ok(true),
    };

    Ok(metadata.previous_generation != latest - 1)
}

fn island_palette() -> Vec<IslandPalette> {
    vec![
        IslandPalette { node: 1, color: "#6FA8FF".to_string() },
        IslandPalette { node: 2, color: "#67D4C0".to_string() },
        IslandPalette { node: 3, color: "#E2B84D".to_string() },
        IslandPalette { node: 4, color: "#D9865C".to_string() },
        IslandPalette { node: 5, color: "#C772A8".to_string() },
        IslandPalette { node: 6, color: "#9B8CFF".to_string() },
    ]
}

async fn load_population_records(
    pool: &Pool,
    previous_generation: i32,
) -> Result<Vec<PopulationRecord>, Box<dyn Error>> {
    let client = pool.get().await?;
    let rows = client.query(
        r#"
        SELECT
            s.song_id,
            s.generation,
            s.node,
            COALESCE(pgf.sum_of_ratings, hfs.sum_of_ratings, 0) AS sum_of_ratings
        FROM songs s
        LEFT JOIN previous_generation_fitness pgf
            ON pgf.song_id = s.song_id AND pgf.generation = s.generation
        LEFT JOIN historic_fitness_scores hfs
            ON hfs.song_id = s.song_id
        WHERE s.generation = $1
        ORDER BY s.node, COALESCE(pgf.sum_of_ratings, hfs.sum_of_ratings, 0) DESC, s.song_id
        "#,
        &[&previous_generation],
    ).await?;

    Ok(rows.into_iter().map(|row| PopulationRecord {
        song_id: row.get("song_id"),
        generation: row.get("generation"),
        node: row.get("node"),
        sum_of_ratings: row.get("sum_of_ratings"),
    }).collect())
}

fn select_spotlights(population: &[PopulationRecord]) -> Vec<PopulationRecord> {
    let mut sorted = population.to_vec();
    sorted.sort_by(|a, b| {
        b.sum_of_ratings
            .cmp(&a.sum_of_ratings)
            .then(a.node.cmp(&b.node))
            .then(a.song_id.cmp(&b.song_id))
    });

    let mut chosen = Vec::new();
    let mut used_nodes = BTreeSet::new();
    for song in sorted {
        if used_nodes.insert(song.node) {
            chosen.push(song);
        }
    }
    chosen
}

async fn load_song_records(
    pool: &Pool,
    min_generation: i32,
    max_generation: i32,
) -> Result<HashMap<i32, SongRecord>, Box<dyn Error>> {
    let client = pool.get().await?;
    let rows = client.query(
        r#"
        SELECT song_id, generation, node, parent1_id, parent2_id, genome
        FROM songs
        WHERE generation BETWEEN $1 AND $2
        "#,
        &[&min_generation, &max_generation],
    ).await?;

    Ok(rows.into_iter().map(|row| {
        let record = SongRecord {
            song_id: row.get("song_id"),
            generation: row.get("generation"),
            node: row.get("node"),
            parent1_id: row.get("parent1_id"),
            parent2_id: row.get("parent2_id"),
            genome: row.get("genome"),
        };
        (record.song_id, record)
    }).collect())
}

fn collect_parent_ids(record: &SongRecord) -> BTreeSet<i32> {
    [record.parent1_id, record.parent2_id]
        .into_iter()
        .flatten()
        .collect()
}

fn collect_grandparent_ids(records: &HashMap<i32, SongRecord>, record: &SongRecord) -> BTreeSet<i32> {
    let mut grandparents = BTreeSet::new();
    for parent_id in collect_parent_ids(record) {
        if let Some(parent) = records.get(&parent_id) {
            grandparents.extend(collect_parent_ids(parent));
        }
    }
    grandparents
}

fn build_spotlight_tree(
    spotlight: &PopulationRecord,
    records: &HashMap<i32, SongRecord>,
) -> Result<SpotlightTree, Box<dyn Error>> {
    let spotlight_record = records
        .get(&spotlight.song_id)
        .ok_or_else(|| format!("Missing spotlight record {}", spotlight.song_id))?;

    let spotlight_parents = collect_parent_ids(spotlight_record);
    let spotlight_grandparents = collect_grandparent_ids(records, spotlight_record);

    let use_grandparent_rule = !spotlight_grandparents.is_empty();
    let mut generation_nodes = BTreeSet::new();
    generation_nodes.insert(spotlight.song_id);

    for record in records.values().filter(|r| r.generation == spotlight.generation) {
        let is_relative = if use_grandparent_rule {
            let grandparents = collect_grandparent_ids(records, record);
            !grandparents.is_disjoint(&spotlight_grandparents)
        } else {
            let parents = collect_parent_ids(record);
            !parents.is_disjoint(&spotlight_parents)
        };

        if is_relative {
            generation_nodes.insert(record.song_id);
        }
    }

    let parent_nodes: BTreeSet<i32> = if use_grandparent_rule {
        records
            .values()
            .filter(|record| record.generation == spotlight.generation - 1)
            .filter(|record| {
                let parent_ids = collect_parent_ids(record);
                !parent_ids.is_disjoint(&spotlight_grandparents)
            })
            .map(|record| record.song_id)
            .collect()
    } else {
        spotlight_parents.iter().copied().collect()
    };

    let grandparent_nodes: BTreeSet<i32> = if use_grandparent_rule {
        spotlight_grandparents.iter().copied().collect()
    } else {
        BTreeSet::new()
    };

    let mut visible_nodes = BTreeSet::new();
    visible_nodes.extend(generation_nodes.iter().copied());
    visible_nodes.extend(parent_nodes.iter().copied());
    visible_nodes.extend(grandparent_nodes.iter().copied());

    let spotlight_parent_ids = spotlight_parents;
    let spotlight_grandparent_ids = spotlight_grandparents;

    let mut row_nodes: HashMap<i32, Vec<&SongRecord>> = HashMap::new();
    for song_id in &visible_nodes {
        if let Some(record) = records.get(song_id) {
            row_nodes.entry(record.generation).or_default().push(record);
        }
    }

    for nodes in row_nodes.values_mut() {
        nodes.sort_by(|a, b| a.node.cmp(&b.node).then(a.song_id.cmp(&b.song_id)));
    }

    let mut nodes = Vec::new();
    for (generation, records_in_row) in &row_nodes {
        let count = records_in_row.len().max(1);
        for (index, record) in records_in_row.iter().enumerate() {
            let role = if record.song_id == spotlight.song_id {
                "spotlight"
            } else if *generation == spotlight.generation {
                let parents = collect_parent_ids(record);
                if !parents.is_disjoint(&spotlight_parent_ids) {
                    "sibling"
                } else {
                    "cousin"
                }
            } else if *generation == spotlight.generation - 1 {
                if spotlight_parent_ids.contains(&record.song_id) {
                    "parent"
                } else {
                    "aunt_uncle"
                }
            } else if *generation == spotlight.generation - 2 || *generation == 0 {
                if spotlight_grandparent_ids.contains(&record.song_id) || spotlight.generation <= 2 {
                    "grandparent"
                } else {
                    "ancestor"
                }
            } else {
                "relative"
            };

            let similarity = relatedness::genome_similarity(&spotlight_record.genome, &record.genome);
            let x = if count == 1 {
                0.5
            } else {
                index as f32 / (count - 1) as f32
            };

            nodes.push(TreeNode {
                song_id: record.song_id,
                generation: record.generation,
                node: record.node,
                role: role.to_string(),
                spotlight_similarity: similarity,
                x,
            });
        }
    }

    let visible_lookup: BTreeSet<i32> = visible_nodes;
    let mut edges = Vec::new();
    for song_id in &generation_nodes {
        if let Some(child) = records.get(song_id) {
            for parent_id in [child.parent1_id, child.parent2_id].into_iter().flatten() {
                if visible_lookup.contains(&parent_id) {
                    if let Some(parent) = records.get(&parent_id) {
                        edges.push(TreeEdge {
                            parent_song_id: parent_id,
                            child_song_id: child.song_id,
                            similarity: relatedness::genome_similarity(&parent.genome, &child.genome),
                        });
                    }
                }
            }
        }
    }
    for song_id in &parent_nodes {
        if let Some(child) = records.get(song_id) {
            for parent_id in [child.parent1_id, child.parent2_id].into_iter().flatten() {
                if visible_lookup.contains(&parent_id) {
                    if let Some(parent) = records.get(&parent_id) {
                        edges.push(TreeEdge {
                            parent_song_id: parent_id,
                            child_song_id: child.song_id,
                            similarity: relatedness::genome_similarity(&parent.genome, &child.genome),
                        });
                    }
                }
            }
        }
    }

    let mut adjacency: HashMap<i32, Vec<i32>> = HashMap::new();
    for edge in &edges {
        adjacency.entry(edge.parent_song_id).or_default().push(edge.child_song_id);
        adjacency.entry(edge.child_song_id).or_default().push(edge.parent_song_id);
    }

    let mut reveal_paths = Vec::new();
    for node in &nodes {
        if node.song_id == spotlight.song_id || node.role == "parent" || node.role == "grandparent" {
            continue;
        }
        if let Some(path) = shortest_path(&adjacency, node.song_id, spotlight.song_id) {
            reveal_paths.push(RevealPath {
                target_song_id: node.song_id,
                node_path: path,
            });
        }
    }

    Ok(SpotlightTree {
        spotlight_song_id: spotlight.song_id,
        previous_generation: spotlight.generation,
        nodes,
        edges,
        reveal_paths,
    })
}

fn shortest_path(adjacency: &HashMap<i32, Vec<i32>>, start: i32, goal: i32) -> Option<Vec<i32>> {
    let mut queue = VecDeque::new();
    let mut previous: HashMap<i32, Option<i32>> = HashMap::new();

    queue.push_back(start);
    previous.insert(start, None);

    while let Some(current) = queue.pop_front() {
        if current == goal {
            let mut path = Vec::new();
            let mut cursor = Some(goal);
            while let Some(node) = cursor {
                path.push(node);
                cursor = previous.get(&node).copied().flatten();
            }
            path.reverse();
            return Some(path);
        }

        for neighbour in adjacency.get(&current).into_iter().flatten() {
            if previous.contains_key(neighbour) {
                continue;
            }
            previous.insert(*neighbour, Some(current));
            queue.push_back(*neighbour);
        }
    }

    None
}

fn write_metadata(slot: &Path, metadata: &FamilyTreesMetadata) -> Result<(), Box<dyn Error>> {
    fs::write(slot.join(METADATA_FILENAME), serde_json::to_string_pretty(metadata)?)?;
    Ok(())
}

fn write_spotlight(slot: &Path, index: usize, tree: &SpotlightTree) -> Result<(), Box<dyn Error>> {
    let path = slot.join(SPOTS_SUBDIR).join(format!("{}.json", index));
    fs::write(path, serde_json::to_string_pretty(tree)?)?;
    Ok(())
}

fn generate_wav_to_slot(slot: &Path, song_id: i32, genome: &Genome) -> Result<(), Box<dyn Error>> {
    let decoded = DecodedGenome::decode(genome);
    let destination = slot.join(AUDIO_SUBDIR).join(format!("{}.wav", song_id));
    play_genes::generate_wav(&decoded, destination.to_str().unwrap())?;
    Ok(())
}

pub async fn update_family_trees(
    pool: &Pool,
    previous_generation: i32,
) -> Result<(), Box<dyn Error>> {
    if previous_generation < 1 {
        return Ok(());
    }

    let population = load_population_records(pool, previous_generation).await?;
    if population.is_empty() {
        return Ok(());
    }

    init_dirs()?;

    let min_generation = (previous_generation - 2).max(0);
    let records = load_song_records(pool, min_generation, previous_generation).await?;
    let spotlights = select_spotlights(&population);

    let slot_name = inactive_slot_name();
    let slot = prepare_slot(slot_name)?;

    let spotlight_set: BTreeSet<i32> = spotlights.iter().map(|song| song.song_id).collect();
    let metadata = FamilyTreesMetadata {
        previous_generation,
        built_at: Utc::now().to_rfc3339(),
        islands: island_palette(),
        population: population
            .iter()
            .map(|song| PopulationSong {
                song_id: song.song_id,
                generation: song.generation,
                node: song.node,
                sum_of_ratings: song.sum_of_ratings,
                is_spotlight: spotlight_set.contains(&song.song_id),
            })
            .collect(),
        spotlights: spotlights
            .iter()
            .enumerate()
            .map(|(index, song)| SpotlightSummary {
                index,
                song_id: song.song_id,
                node: song.node,
                sum_of_ratings: song.sum_of_ratings,
            })
            .collect(),
    };

    let mut required_audio = BTreeSet::new();
    for song in &population {
        required_audio.insert(song.song_id);
    }

    for (index, spotlight) in spotlights.iter().enumerate() {
        let tree = build_spotlight_tree(spotlight, &records)?;
        for node in &tree.nodes {
            required_audio.insert(node.song_id);
        }
        write_spotlight(&slot, index, &tree)?;
    }

    for song_id in required_audio {
        if let Some(record) = records.get(&song_id) {
            generate_wav_to_slot(&slot, song_id, &record.genome)?;
        }
    }

    write_metadata(&slot, &metadata)?;
    activate_slot(slot_name)?;

    let _ = save_status(&FamilyTreesStatus {
        status: "healthy".to_string(),
        last_updated: Some(Utc::now().to_rfc3339()),
        last_error: None,
        previous_generation: Some(previous_generation),
    });

    Ok(())
}

pub async fn update_family_trees_safe(
    pool: &Pool,
    previous_generation: i32,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match update_family_trees(pool, previous_generation).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let message = format!("Family trees update failed: {}", error);
            eprintln!("{}", message);
            let _ = save_status(&FamilyTreesStatus {
                status: "failed".to_string(),
                last_updated: Some(Utc::now().to_rfc3339()),
                last_error: Some(message.clone()),
                previous_generation: Some(previous_generation),
            });
            Err(message.into())
        }
    }
}

pub async fn ensure_family_trees(
    pool: &Pool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let latest = latest_generation(pool).await?;
    if latest < 2 {
        return Ok(());
    }

    if !needs_rebuild(pool).await? {
        return Ok(());
    }

    if REBUILD_IN_PROGRESS.load(Ordering::SeqCst) {
        return Ok(());
    }

    let _guard = match REBUILD_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(_) => return Ok(()),
    };

    if !needs_rebuild(pool).await? {
        return Ok(());
    }

    REBUILD_IN_PROGRESS.store(true, Ordering::SeqCst);
    let previous_generation = latest - 1;

    let _ = save_status(&FamilyTreesStatus {
        status: "rebuilding".to_string(),
        last_updated: Some(Utc::now().to_rfc3339()),
        last_error: None,
        previous_generation: Some(previous_generation),
    });

    let result = update_family_trees_safe(pool, previous_generation).await;
    REBUILD_IN_PROGRESS.store(false, Ordering::SeqCst);
    result
}

pub fn ensure_family_trees_background(pool: Pool) {
    if REBUILD_IN_PROGRESS.load(Ordering::SeqCst) {
        return;
    }

    tokio::spawn(async move {
        if let Err(error) = ensure_family_trees(&pool).await {
            eprintln!("Family trees: Background rebuild failed: {}", error);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn genome(seed: u8) -> Genome {
        let seq = |base: u8| -> Vec<u8> {
            vec![base % 2, (base + 1) % 2, (base + 2) % 2, (base + 3) % 2]
        };

        Genome::new(
            seq(seed),
            seq(seed + 1),
            seq(seed + 2),
            seq(seed + 3),
            seq(seed + 4),
            seq(seed + 5),
            seq(seed + 6),
            seq(seed + 7),
            seq(seed + 8),
            seq(seed + 9),
            seq(seed + 10),
            seq(seed + 11),
            seq(seed + 12),
            seq(seed + 13),
            seq(seed + 14),
            seq(seed + 15),
            seq(seed + 16),
            seq(seed + 17),
            vec![0, 0, 0, 1, 1, 0, 1, 0],
            vec![0, 0, 0, 1, 1, 0, 1, 0],
        )
    }

    fn song_record(
        song_id: i32,
        generation: i32,
        node: i32,
        parent1_id: Option<i32>,
        parent2_id: Option<i32>,
        seed: u8,
    ) -> SongRecord {
        SongRecord {
            song_id,
            generation,
            node,
            parent1_id,
            parent2_id,
            genome: genome(seed),
        }
    }

    #[test]
    fn select_spotlights_is_greedy_and_one_per_island() {
        let population = vec![
            PopulationRecord { song_id: 10, generation: 5, node: 1, sum_of_ratings: 20 },
            PopulationRecord { song_id: 11, generation: 5, node: 1, sum_of_ratings: 18 },
            PopulationRecord { song_id: 20, generation: 5, node: 2, sum_of_ratings: 19 },
            PopulationRecord { song_id: 30, generation: 5, node: 3, sum_of_ratings: 17 },
            PopulationRecord { song_id: 40, generation: 5, node: 4, sum_of_ratings: 16 },
        ];

        let selected = select_spotlights(&population);
        let ids: Vec<i32> = selected.into_iter().map(|song| song.song_id).collect();

        assert_eq!(ids, vec![10, 20, 30, 40]);
    }

    #[test]
    fn build_spotlight_tree_keeps_shared_grandparent_family_only() {
        let spotlight = PopulationRecord { song_id: 7, generation: 3, node: 1, sum_of_ratings: 12 };
        let records = HashMap::from([
            (1, song_record(1, 1, 1, None, None, 1)),
            (2, song_record(2, 1, 2, None, None, 2)),
            (11, song_record(11, 1, 3, None, None, 11)),
            (3, song_record(3, 2, 1, Some(1), Some(2), 3)),
            (4, song_record(4, 2, 2, Some(1), Some(2), 4)),
            (5, song_record(5, 2, 3, Some(11), None, 5)),
            (6, song_record(6, 2, 4, Some(11), None, 6)),
            (7, song_record(7, 3, 1, Some(3), None, 7)),
            (8, song_record(8, 3, 1, Some(3), None, 8)),
            (9, song_record(9, 3, 2, Some(4), None, 9)),
            (10, song_record(10, 3, 3, Some(5), Some(6), 10)),
        ]);

        let tree = build_spotlight_tree(&spotlight, &records).unwrap();
        let visible: BTreeSet<i32> = tree.nodes.iter().map(|node| node.song_id).collect();

        assert!(visible.contains(&7));
        assert!(visible.contains(&8));
        assert!(visible.contains(&9));
        assert!(visible.contains(&3));
        assert!(visible.contains(&4));
        assert!(visible.contains(&1));
        assert!(visible.contains(&2));
        assert!(!visible.contains(&10));
        assert!(!visible.contains(&5));
        assert!(!visible.contains(&6));
        assert_eq!(
            tree.nodes.iter().filter(|node| node.role == "grandparent").count(),
            2
        );

        let cousin_path = tree
            .reveal_paths
            .iter()
            .find(|path| path.target_song_id == 9)
            .expect("cousin path should exist");
        assert_eq!(cousin_path.node_path.first().copied(), Some(9));
        assert_eq!(cousin_path.node_path.last().copied(), Some(7));
        assert!(cousin_path.node_path.contains(&4));
        assert!(cousin_path.node_path.contains(&1) || cousin_path.node_path.contains(&2));
        assert!(cousin_path.node_path.contains(&3));
    }

    #[test]
    fn build_spotlight_tree_uses_shared_parent_rule_when_grandparents_absent() {
        let spotlight = PopulationRecord { song_id: 5, generation: 2, node: 1, sum_of_ratings: 8 };
        let records = HashMap::from([
            (1, song_record(1, 1, 1, None, None, 1)),
            (2, song_record(2, 1, 2, None, None, 2)),
            (5, song_record(5, 2, 1, Some(1), None, 5)),
            (6, song_record(6, 2, 1, Some(1), None, 6)),
            (7, song_record(7, 2, 2, Some(2), None, 7)),
        ]);

        let tree = build_spotlight_tree(&spotlight, &records).unwrap();
        let visible: BTreeSet<i32> = tree.nodes.iter().map(|node| node.song_id).collect();

        assert!(visible.contains(&5));
        assert!(visible.contains(&6));
        assert!(visible.contains(&1));
        assert!(!visible.contains(&7));
    }
}
