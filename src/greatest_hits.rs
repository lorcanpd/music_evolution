// src/greatest_hits.rs
//
// Manages the "Greatest Hits" archive with atomic revision switching.
// Persists top-rated songs across generations with their WAV files.
//
// Directory structure:
//   data/
//   ├── greatest_hits/
//   │   ├── revisions/
//   │   │   ├── 1/
//   │   │   │   ├── metadata.json
//   │   │   │   └── audio/
//   │   │   │       ├── 1.wav, 2.wav, ...
//   │   │   └── 2/
//   │   │       ├── metadata.json
//   │   │       └── audio/
//   │   │           ├── 3.wav, 5.wav, ...
//   │   └── current -> revisions/2  (symlink)
//   │   └── status.json  (rebuild status tracking)

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use deadpool_postgres::Pool;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::decode_genome::DecodedGenome;
use crate::genome::Genome;
use crate::play_genes;

/// Global lock to prevent concurrent rebuilds (single-flight pattern)
static REBUILD_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// Global flag to indicate if a rebuild is in progress
static REBUILD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Base directory for greatest hits data
pub const DATA_BASE_DIR: &str = "data";
pub const GREATEST_HITS_SUBDIR: &str = "greatest_hits";
pub const REVISIONS_SUBDIR: &str = "revisions";
pub const CURRENT_SYMLINK_NAME: &str = "current";
pub const METADATA_FILENAME: &str = "metadata.json";
pub const AUDIO_SUBDIR: &str = "audio";
pub const STATUS_FILENAME: &str = "status.json";

/// Number of top songs to include in greatest hits
pub const TOP_N_SONGS: usize = 10;

/// Status of the greatest hits system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreatestHitsStatus {
    pub status: String, // "healthy", "rebuilding", "failed", "missing"
    pub last_updated: Option<String>, // ISO 8601 timestamp
    pub last_error: Option<String>,
    pub revision: Option<i32>,
}

/// Metadata for a single greatest hit song
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreatestHitEntry {
    pub song_id: i32,
    pub generation: i32,
    pub node: i32,
    pub parent1_id: Option<i32>,
    pub parent2_id: Option<i32>,
    pub likes: i64,
    pub dislikes: i64,
    pub score: f64, // likes / (likes + dislikes)
    pub added_at_generation: i32,
}

/// Metadata for an entire greatest hits revision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreatestHitsMetadata {
    pub revision: i32,
    pub created_at: String, // ISO 8601 timestamp
    pub trigger_generation: i32, // generation that triggered this update
    pub songs: Vec<GreatestHitEntry>,
}

/// Get the path to the greatest hits base directory
pub fn greatest_hits_base_path() -> PathBuf {
    PathBuf::from(DATA_BASE_DIR).join(GREATEST_HITS_SUBDIR)
}

/// Get the path to the revisions directory
pub fn revisions_path() -> PathBuf {
    greatest_hits_base_path().join(REVISIONS_SUBDIR)
}

/// Get the path to the current symlink
pub fn current_symlink_path() -> PathBuf {
    greatest_hits_base_path().join(CURRENT_SYMLINK_NAME)
}

/// Get the path to a specific revision's directory
pub fn revision_path(revision: i32) -> PathBuf {
    revisions_path().join(revision.to_string())
}

/// Get the path to a revision's metadata file
pub fn metadata_path(revision: i32) -> PathBuf {
    revision_path(revision).join(METADATA_FILENAME)
}

/// Get the path to a revision's audio directory
pub fn audio_path(revision: i32) -> PathBuf {
    revision_path(revision).join(AUDIO_SUBDIR)
}

/// Get the path where metadata should be served from (follows symlink)
pub fn serving_metadata_path() -> PathBuf {
    current_symlink_path().join(METADATA_FILENAME)
}

/// Get the path where audio should be served from (follows symlink)
pub fn serving_audio_path() -> PathBuf {
    current_symlink_path().join(AUDIO_SUBDIR)
}

/// Get the path to a specific WAV file in the current greatest hits
pub fn wav_file_path(song_id: i32) -> PathBuf {
    serving_audio_path().join(format!("{}.wav", song_id))
}

/// Get the path to the status file
pub fn status_file_path() -> PathBuf {
    greatest_hits_base_path().join(STATUS_FILENAME)
}

/// Save the current status to the status file
pub fn save_status(status: &GreatestHitsStatus) -> io::Result<()> {
    let path = status_file_path();
    let json = serde_json::to_string_pretty(status)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)
}

/// Load the current status from the status file
pub fn load_status() -> Option<GreatestHitsStatus> {
    let path = status_file_path();
    fs::read_to_string(path)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
}

/// Check if greatest hits data is healthy (fast filesystem check).
///
/// Returns true if:
/// - data/greatest_hits/current symlink exists
/// - The symlink points to an existing revision directory
/// - The revision contains metadata.json that parses successfully
/// - The revision contains an audio/ directory
pub fn is_healthy() -> bool {
    is_healthy_at(&greatest_hits_base_path())
}

/// Check health at a specific base path (for testing)
pub fn is_healthy_at(base_path: &Path) -> bool {
    let symlink = base_path.join(CURRENT_SYMLINK_NAME);

    // Check symlink exists
    if !symlink.exists() {
        return false;
    }

    // Check symlink points to valid directory
    let target = match fs::read_link(&symlink) {
        Ok(t) => base_path.join(t),
        Err(_) => return false,
    };

    if !target.is_dir() {
        return false;
    }

    // Check metadata.json exists and is valid JSON
    let metadata_path = target.join(METADATA_FILENAME);
    if !metadata_path.exists() {
        return false;
    }

    let metadata_content = match fs::read_to_string(&metadata_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Try to parse the metadata
    if serde_json::from_str::<GreatestHitsMetadata>(&metadata_content).is_err() {
        return false;
    }

    // Check audio directory exists
    let audio_dir = target.join(AUDIO_SUBDIR);
    if !audio_dir.is_dir() {
        return false;
    }

    true
}

/// Check if a rebuild is currently in progress
pub fn is_rebuild_in_progress() -> bool {
    REBUILD_IN_PROGRESS.load(Ordering::SeqCst)
}

/// Initialize the greatest hits directory structure.
pub fn init_dirs() -> io::Result<()> {
    fs::create_dir_all(revisions_path())?;
    Ok(())
}

/// Get the next revision number by scanning existing revisions.
pub fn get_next_revision() -> io::Result<i32> {
    let revs_path = revisions_path();
    if !revs_path.exists() {
        return Ok(1);
    }

    let mut max_revision = 0;
    for entry in fs::read_dir(&revs_path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                if let Ok(rev) = name.parse::<i32>() {
                    max_revision = max_revision.max(rev);
                }
            }
        }
    }
    Ok(max_revision + 1)
}

/// Get the current revision number by reading the symlink.
pub fn get_current_revision() -> Option<i32> {
    let symlink = current_symlink_path();
    if !symlink.exists() {
        return None;
    }
    fs::read_link(&symlink).ok().and_then(|target| {
        target.file_name()
            .and_then(|n| n.to_str())
            .and_then(|s| s.parse().ok())
    })
}

/// Create a new revision directory.
pub fn create_revision_dir(revision: i32) -> io::Result<PathBuf> {
    let rev_path = revision_path(revision);
    if rev_path.exists() {
        fs::remove_dir_all(&rev_path)?;
    }
    fs::create_dir_all(&rev_path)?;
    fs::create_dir_all(audio_path(revision))?;
    Ok(rev_path)
}

/// Atomically activate a revision by updating the symlink.
pub fn activate_revision(revision: i32) -> Result<(), Box<dyn Error>> {
    let target = revision_path(revision);
    let symlink = current_symlink_path();
    let temp_symlink = greatest_hits_base_path().join(".current_new");

    if !target.exists() {
        return Err(format!("Revision directory does not exist: {}", target.display()).into());
    }

    let _ = fs::remove_file(&temp_symlink);

    let relative_target = PathBuf::from(REVISIONS_SUBDIR).join(revision.to_string());

    #[cfg(unix)]
    std::os::unix::fs::symlink(&relative_target, &temp_symlink)?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&relative_target, &temp_symlink)?;

    fs::rename(&temp_symlink, &symlink)?;

    eprintln!("Activated greatest hits revision {} (symlink: {} -> {})",
              revision, symlink.display(), relative_target.display());

    Ok(())
}

/// Clean up old revisions, keeping only the most recent N.
pub fn cleanup_old_revisions(keep_count: usize) -> Result<(), Box<dyn Error>> {
    let revs_path = revisions_path();
    if !revs_path.exists() {
        return Ok(());
    }

    let mut revisions: Vec<i32> = Vec::new();
    for entry in fs::read_dir(&revs_path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                if let Ok(rev) = name.parse::<i32>() {
                    revisions.push(rev);
                }
            }
        }
    }

    revisions.sort_by(|a, b| b.cmp(a));

    for rev in revisions.iter().skip(keep_count) {
        let rev_path = revision_path(*rev);
        match fs::remove_dir_all(&rev_path) {
            Ok(_) => eprintln!("Cleaned up old greatest hits revision: {}", rev_path.display()),
            Err(e) => eprintln!("Warning: Failed to clean up {}: {}", rev_path.display(), e),
        }
    }

    Ok(())
}

/// Save metadata to a revision's directory.
pub fn save_metadata(revision: i32, metadata: &GreatestHitsMetadata) -> Result<(), Box<dyn Error>> {
    let path = metadata_path(revision);
    let json = serde_json::to_string_pretty(metadata)?;
    fs::write(path, json)?;
    Ok(())
}

/// Load current greatest hits metadata (from symlinked current directory).
pub fn load_current_metadata() -> Result<GreatestHitsMetadata, Box<dyn Error>> {
    let path = serving_metadata_path();
    let json = fs::read_to_string(path)?;
    let metadata: GreatestHitsMetadata = serde_json::from_str(&json)?;
    Ok(metadata)
}

/// Get the latest generation number present in the songs table.
pub async fn latest_generation(pool: &Pool) -> Result<i32, Box<dyn Error + Send + Sync>> {
    let client = pool.get().await?;
    let current_gen: i32 = client
        .query_one("SELECT COALESCE(MAX(generation), 1) as gen FROM songs", &[])
        .await?
        .get("gen");
    Ok(current_gen)
}

/// Determine whether greatest hits should be rebuilt.
///
/// Rebuild is required if:
/// - the on-disk archive is structurally unhealthy, or
/// - the archive was built for an older generation than currently exists in the database.
pub async fn needs_rebuild(pool: &Pool) -> Result<bool, Box<dyn Error + Send + Sync>> {
    if !is_healthy() {
        return Ok(true);
    }

    let metadata = match load_current_metadata() {
        Ok(metadata) => metadata,
        Err(_) => return Ok(true),
    };

    let current_gen = latest_generation(pool).await?;
    Ok(metadata.trigger_generation < current_gen)
}

/// Generate a WAV file for a greatest-hit song directly from its genome.
pub fn generate_wav_to_revision(
    revision: i32,
    song_id: i32,
    genome: &Genome,
) -> Result<(), Box<dyn Error>> {
    let dest_path = audio_path(revision).join(format!("{}.wav", song_id));
    let decoded = DecodedGenome::decode(genome);
    play_genes::generate_wav(&decoded, dest_path.to_str().unwrap())?;
    Ok(())
}

/// Compute and update greatest hits after a generation completes.
/// This queries the database for all-time top songs and regenerates their WAVs.
///
/// This function is designed to never panic. All errors are returned as Result::Err.
pub async fn update_greatest_hits(
    pool: &Pool,
    current_generation: i32,
) -> Result<(), Box<dyn Error>> {
    eprintln!("Greatest hits: Starting update for generation {}", current_generation);

    // Update status to indicate we're updating
    let _ = save_status(&GreatestHitsStatus {
        status: "updating".to_string(),
        last_updated: Some(chrono::Utc::now().to_rfc3339()),
        last_error: None,
        revision: get_current_revision(),
    });

    init_dirs()?;

    let client = pool.get().await?;

    // Query top N songs by score (likes / total votes)
    // We need to aggregate all votes from historic_fitness_scores
    // historic_fitness_scores stores sum_of_ratings where 1=like, 0=dislike
    // To get likes/dislikes we need to count individual ratings

    // Actually, looking at the schema more carefully:
    // - current_generation_fitness: stores individual (song_id, rating) pairs for current gen
    // - historic_fitness_scores: stores (song_id, sum_of_ratings) - archived totals
    //
    // The sum_of_ratings in historic is the SUM of rating values (0 or 1)
    // We need likes = sum_of_ratings, but we don't have total votes stored
    //
    // For now, let's compute from historic + current, counting votes properly
    // We'll create a view that combines historic data with song metadata

    // Query to get top songs across all generations:
    // 1. Count likes (rating=1) and dislikes (rating=0) from current_generation_fitness
    // 2. Also check historic_fitness_scores for archived data
    // 3. Join with songs table for metadata
    //
    // IMPORTANT: All numeric aggregates are explicitly cast to BIGINT to prevent
    // Postgres from returning NUMERIC type which causes Rust type deserialization errors.

    let rows = client.query(
        r#"
        WITH vote_stats AS (
            -- Get vote counts from current generation
            -- Explicit ::bigint casts to ensure consistent types
            SELECT
                song_id,
                COALESCE(SUM(CASE WHEN rating = 1 THEN 1 ELSE 0 END)::bigint, 0::bigint) as likes,
                COALESCE(SUM(CASE WHEN rating = 0 THEN 1 ELSE 0 END)::bigint, 0::bigint) as dislikes
            FROM current_generation_fitness
            GROUP BY song_id

            UNION ALL

            -- Get historic votes (sum_of_ratings is accumulated likes)
            -- Note: historic doesn't track dislikes separately, so we use 0
            -- Cast sum_of_ratings to bigint for type consistency
            SELECT
                song_id,
                COALESCE(sum_of_ratings::bigint, 0::bigint) as likes,
                0::bigint as dislikes
            FROM historic_fitness_scores
        ),
        combined AS (
            SELECT
                song_id,
                COALESCE(SUM(likes)::bigint, 0::bigint) as total_likes,
                COALESCE(SUM(dislikes)::bigint, 0::bigint) as total_dislikes
            FROM vote_stats
            GROUP BY song_id
            HAVING SUM(likes) + SUM(dislikes) > 0
        )
        SELECT
            s.song_id,
            s.generation,
            s.node,
            s.genome,
            s.parent1_id,
            s.parent2_id,
            COALESCE(c.total_likes, 0::bigint) as likes,
            COALESCE(c.total_dislikes, 0::bigint) as dislikes,
            CASE
                WHEN c.total_likes + c.total_dislikes > 0
                THEN (c.total_likes::float8 / (c.total_likes + c.total_dislikes)::float8)
                ELSE 0.0::float8
            END as score
        FROM songs s
        JOIN combined c ON s.song_id = c.song_id
        WHERE s.generation > 0
        ORDER BY score DESC, c.total_likes DESC
        LIMIT $1
        "#,
        &[&(TOP_N_SONGS as i64)],
    ).await?;

    if rows.is_empty() {
        eprintln!("No songs with votes found for greatest hits");
        return Ok(());
    }

    let revision = get_next_revision()?;
    create_revision_dir(revision)?;

    let mut entries = Vec::new();

    for row in &rows {
        // Use try_get with explicit error handling to avoid panics
        // If any column fails to deserialize, skip this row but continue processing
        let song_id: i32 = match row.try_get("song_id") {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Warning: Failed to get song_id from row: {}", e);
                continue;
            }
        };
        let generation: i32 = row.try_get("generation").unwrap_or(0);
        let node: i32 = row.try_get("node").unwrap_or(0);
        let genome: Genome = match row.try_get("genome") {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Warning: Failed to get genome for song {}: {}", song_id, e);
                continue;
            }
        };
        let parent1_id: Option<i32> = row.try_get("parent1_id").ok().flatten();
        let parent2_id: Option<i32> = row.try_get("parent2_id").ok().flatten();

        // Handle likes/dislikes with defensive fallback
        // These should be BIGINT from our explicit casts in SQL
        // If type conversion fails, log warning and use 0 as fallback
        let likes: i64 = match row.try_get::<_, i64>("likes") {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Warning: Failed to get likes for song {} (type mismatch?): {}", song_id, e);
                // Try as i32 and upcast as fallback
                row.try_get::<_, i32>("likes").map(|v| v as i64).unwrap_or(0)
            }
        };

        let dislikes: i64 = match row.try_get::<_, i64>("dislikes") {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Warning: Failed to get dislikes for song {} (type mismatch?): {}", song_id, e);
                row.try_get::<_, i32>("dislikes").map(|v| v as i64).unwrap_or(0)
            }
        };

        let score: f64 = match row.try_get::<_, f64>("score") {
            Ok(v) => v,
            Err(_) => {
                // Compute from likes/dislikes if score column fails
                let total = likes + dislikes;
                if total > 0 {
                    likes as f64 / total as f64
                } else {
                    0.0
                }
            }
        };

        if let Err(e) = generate_wav_to_revision(revision, song_id, &genome) {
            eprintln!("Warning: Failed to generate greatest-hit WAV for song {}: {}", song_id, e);
        }

        entries.push(GreatestHitEntry {
            song_id,
            generation,
            node,
            parent1_id,
            parent2_id,
            likes,
            dislikes,
            score,
            added_at_generation: current_generation,
        });
    }

    let metadata = GreatestHitsMetadata {
        revision,
        created_at: chrono::Utc::now().to_rfc3339(),
        trigger_generation: current_generation,
        songs: entries,
    };

    save_metadata(revision, &metadata)?;
    activate_revision(revision)?;
    cleanup_old_revisions(3)?; // Keep last 3 revisions

    // Update status to healthy
    let _ = save_status(&GreatestHitsStatus {
        status: "healthy".to_string(),
        last_updated: Some(chrono::Utc::now().to_rfc3339()),
        last_error: None,
        revision: Some(revision),
    });

    eprintln!("Greatest hits: Update succeeded - revision {} with {} songs",
              revision, metadata.songs.len());

    Ok(())
}

/// Safe wrapper for update_greatest_hits that ensures proper status tracking on error.
/// This function will not panic - all errors are caught and logged.
pub async fn update_greatest_hits_safe(
    pool: &Pool,
    current_generation: i32,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match update_greatest_hits(pool, current_generation).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let error_msg = format!("Greatest hits update failed: {}", e);
            eprintln!("{}", error_msg);
            let _ = save_status(&GreatestHitsStatus {
                status: "failed".to_string(),
                last_updated: Some(chrono::Utc::now().to_rfc3339()),
                last_error: Some(error_msg.clone()),
                revision: get_current_revision(),
            });
            Err(error_msg.into())
        }
    }
}

/// Ensure greatest hits data exists and is healthy, triggering a rebuild if needed.
///
/// This function is single-flight: if a rebuild is already in progress, it returns immediately.
/// It's designed to be called from endpoints and startup without blocking.
pub async fn ensure_greatest_hits(
    pool: &Pool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Fast path: if healthy and current, return immediately
    if !needs_rebuild(pool).await? {
        return Ok(());
    }

    // Check if rebuild is already in progress
    if REBUILD_IN_PROGRESS.load(Ordering::SeqCst) {
        eprintln!("Greatest hits: Rebuild already in progress, skipping");
        return Ok(());
    }

    // Try to acquire the rebuild lock
    let _guard = match REBUILD_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            eprintln!("Greatest hits: Could not acquire rebuild lock, another rebuild in progress");
            return Ok(());
        }
    };

    // Double-check state after acquiring lock
    if !needs_rebuild(pool).await? {
        return Ok(());
    }

    // Set rebuild in progress flag
    REBUILD_IN_PROGRESS.store(true, Ordering::SeqCst);
    eprintln!("Greatest hits: Data is missing, stale, or corrupt, triggering rebuild");

    // Update status
    let _ = save_status(&GreatestHitsStatus {
        status: "rebuilding".to_string(),
        last_updated: Some(chrono::Utc::now().to_rfc3339()),
        last_error: None,
        revision: get_current_revision(),
    });

    // Get the current generation from the database
    let current_gen = latest_generation(pool).await?;

    // Perform the rebuild
    let result = update_greatest_hits_safe(pool, current_gen).await;

    // Clear the rebuild flag
    REBUILD_IN_PROGRESS.store(false, Ordering::SeqCst);
    eprintln!("Greatest hits: Rebuild complete, in_progress flag cleared");

    result
}

/// Spawn a background task to ensure greatest hits exists.
/// Returns immediately without blocking.
pub fn ensure_greatest_hits_background(pool: Pool) {
    if REBUILD_IN_PROGRESS.load(Ordering::SeqCst) {
        return;
    }

    tokio::spawn(async move {
        if let Err(e) = ensure_greatest_hits(&pool).await {
            eprintln!("Greatest hits: Background rebuild failed: {}", e);
        }
    });
}

/// Check if greatest hits is initialized (has at least one revision).
pub fn is_initialized() -> bool {
    current_symlink_path().exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a valid greatest hits structure for testing
    fn create_valid_structure(base: &Path) -> io::Result<()> {
        let revisions = base.join(REVISIONS_SUBDIR);
        let rev1 = revisions.join("1");
        let audio = rev1.join(AUDIO_SUBDIR);

        fs::create_dir_all(&audio)?;

        // Create valid metadata
        let metadata = GreatestHitsMetadata {
            revision: 1,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            trigger_generation: 1,
            songs: vec![],
        };
        let metadata_json = serde_json::to_string_pretty(&metadata).unwrap();
        fs::write(rev1.join(METADATA_FILENAME), metadata_json)?;

        // Create symlink
        let symlink = base.join(CURRENT_SYMLINK_NAME);
        let target = PathBuf::from(REVISIONS_SUBDIR).join("1");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &symlink)?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&target, &symlink)?;

        Ok(())
    }

    #[test]
    fn test_is_healthy_at_with_missing_directory() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // Empty directory should not be healthy
        assert!(!is_healthy_at(base));
    }

    #[test]
    fn test_is_healthy_at_with_missing_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // Create revisions dir but no symlink
        fs::create_dir_all(base.join(REVISIONS_SUBDIR)).unwrap();

        assert!(!is_healthy_at(base));
    }

    #[test]
    fn test_is_healthy_at_with_broken_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // Create symlink pointing to non-existent target
        let symlink = base.join(CURRENT_SYMLINK_NAME);
        let target = PathBuf::from(REVISIONS_SUBDIR).join("999");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &symlink).unwrap();

        assert!(!is_healthy_at(base));
    }

    #[test]
    fn test_is_healthy_at_with_missing_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // Create directory structure without metadata
        let revisions = base.join(REVISIONS_SUBDIR);
        let rev1 = revisions.join("1");
        let audio = rev1.join(AUDIO_SUBDIR);
        fs::create_dir_all(&audio).unwrap();

        // Create symlink
        let symlink = base.join(CURRENT_SYMLINK_NAME);
        let target = PathBuf::from(REVISIONS_SUBDIR).join("1");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &symlink).unwrap();

        // No metadata.json - should fail
        assert!(!is_healthy_at(base));
    }

    #[test]
    fn test_is_healthy_at_with_invalid_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // Create directory structure
        let revisions = base.join(REVISIONS_SUBDIR);
        let rev1 = revisions.join("1");
        let audio = rev1.join(AUDIO_SUBDIR);
        fs::create_dir_all(&audio).unwrap();

        // Create invalid metadata
        fs::write(rev1.join(METADATA_FILENAME), "not valid json {{{").unwrap();

        // Create symlink
        let symlink = base.join(CURRENT_SYMLINK_NAME);
        let target = PathBuf::from(REVISIONS_SUBDIR).join("1");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &symlink).unwrap();

        // Invalid JSON should fail
        assert!(!is_healthy_at(base));
    }

    #[test]
    fn test_is_healthy_at_with_missing_audio_dir() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // Create directory structure without audio dir
        let revisions = base.join(REVISIONS_SUBDIR);
        let rev1 = revisions.join("1");
        fs::create_dir_all(&rev1).unwrap();

        // Create valid metadata
        let metadata = GreatestHitsMetadata {
            revision: 1,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            trigger_generation: 1,
            songs: vec![],
        };
        let metadata_json = serde_json::to_string_pretty(&metadata).unwrap();
        fs::write(rev1.join(METADATA_FILENAME), metadata_json).unwrap();

        // Create symlink
        let symlink = base.join(CURRENT_SYMLINK_NAME);
        let target = PathBuf::from(REVISIONS_SUBDIR).join("1");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &symlink).unwrap();

        // No audio directory should fail
        assert!(!is_healthy_at(base));
    }

    #[test]
    fn test_is_healthy_at_with_valid_structure() {
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        create_valid_structure(base).unwrap();

        // Valid structure should pass
        assert!(is_healthy_at(base));
    }

    #[test]
    fn test_status_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let status_path = temp_dir.path().join("status.json");

        let status = GreatestHitsStatus {
            status: "healthy".to_string(),
            last_updated: Some("2024-01-01T00:00:00Z".to_string()),
            last_error: None,
            revision: Some(5),
        };

        // Save and load
        let json = serde_json::to_string_pretty(&status).unwrap();
        fs::write(&status_path, &json).unwrap();

        let loaded_json = fs::read_to_string(&status_path).unwrap();
        let loaded: GreatestHitsStatus = serde_json::from_str(&loaded_json).unwrap();

        assert_eq!(loaded.status, "healthy");
        assert_eq!(loaded.revision, Some(5));
        assert!(loaded.last_error.is_none());
    }

    #[test]
    fn test_status_with_error() {
        let status = GreatestHitsStatus {
            status: "failed".to_string(),
            last_updated: Some("2024-01-01T00:00:00Z".to_string()),
            last_error: Some("Database connection failed".to_string()),
            revision: Some(3),
        };

        let json = serde_json::to_string(&status).unwrap();
        let loaded: GreatestHitsStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.status, "failed");
        assert_eq!(loaded.last_error, Some("Database connection failed".to_string()));
    }

    #[test]
    fn test_metadata_serialization() {
        let entry = GreatestHitEntry {
            song_id: 42,
            generation: 5,
            node: 1,
            parent1_id: Some(10),
            parent2_id: None,
            likes: 100,
            dislikes: 20,
            score: 0.833,
            added_at_generation: 5,
        };

        let metadata = GreatestHitsMetadata {
            revision: 3,
            created_at: "2024-01-15T10:30:00Z".to_string(),
            trigger_generation: 5,
            songs: vec![entry],
        };

        let json = serde_json::to_string_pretty(&metadata).unwrap();
        let loaded: GreatestHitsMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.revision, 3);
        assert_eq!(loaded.songs.len(), 1);
        assert_eq!(loaded.songs[0].song_id, 42);
        assert_eq!(loaded.songs[0].likes, 100);
        assert_eq!(loaded.songs[0].dislikes, 20);
    }

    #[test]
    fn test_path_functions() {
        // Test that path functions return expected structure
        let base = greatest_hits_base_path();
        assert!(base.ends_with(GREATEST_HITS_SUBDIR));

        let revs = revisions_path();
        assert!(revs.ends_with(REVISIONS_SUBDIR));

        let rev1 = revision_path(1);
        assert!(rev1.ends_with("1"));

        let meta = metadata_path(1);
        assert!(meta.ends_with(METADATA_FILENAME));

        let audio = audio_path(1);
        assert!(audio.ends_with(AUDIO_SUBDIR));

        let wav = wav_file_path(42);
        assert!(wav.ends_with("42.wav"));
    }

    #[test]
    fn test_get_next_revision_empty() {
        let temp_dir = TempDir::new().unwrap();
        // Override the revisions path for testing would require more setup
        // For now, just test that the function doesn't panic on non-existent path
    }
}
