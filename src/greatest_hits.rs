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

use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use deadpool_postgres::Pool;

/// Base directory for greatest hits data
pub const DATA_BASE_DIR: &str = "data";
pub const GREATEST_HITS_SUBDIR: &str = "greatest_hits";
pub const REVISIONS_SUBDIR: &str = "revisions";
pub const CURRENT_SYMLINK_NAME: &str = "current";
pub const METADATA_FILENAME: &str = "metadata.json";
pub const AUDIO_SUBDIR: &str = "audio";

/// Number of top songs to include in greatest hits
pub const TOP_N_SONGS: usize = 10;

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

/// Copy a WAV file from source to the revision's audio directory.
pub fn copy_wav_to_revision(revision: i32, song_id: i32, source_path: &PathBuf) -> io::Result<()> {
    let dest_path = audio_path(revision).join(format!("{}.wav", song_id));
    fs::copy(source_path, dest_path)?;
    Ok(())
}

/// Compute and update greatest hits after a generation completes.
/// This queries the database for all-time top songs and copies their WAVs.
pub async fn update_greatest_hits(
    pool: &Pool,
    current_generation: i32,
) -> Result<(), Box<dyn Error>> {
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

    let rows = client.query(
        r#"
        WITH vote_stats AS (
            -- Get vote counts from current generation
            SELECT
                song_id,
                SUM(CASE WHEN rating = 1 THEN 1 ELSE 0 END) as likes,
                SUM(CASE WHEN rating = 0 THEN 1 ELSE 0 END) as dislikes
            FROM current_generation_fitness
            GROUP BY song_id

            UNION ALL

            -- Get historic votes (sum_of_ratings is accumulated likes)
            -- Note: historic doesn't track dislikes separately, so we estimate
            SELECT
                song_id,
                sum_of_ratings as likes,
                0::bigint as dislikes
            FROM historic_fitness_scores
        ),
        combined AS (
            SELECT
                song_id,
                SUM(likes) as total_likes,
                SUM(dislikes) as total_dislikes
            FROM vote_stats
            GROUP BY song_id
            HAVING SUM(likes) + SUM(dislikes) > 0
        )
        SELECT
            s.song_id,
            s.generation,
            s.node,
            s.parent1_id,
            s.parent2_id,
            c.total_likes as likes,
            c.total_dislikes as dislikes,
            CASE
                WHEN c.total_likes + c.total_dislikes > 0
                THEN c.total_likes::float / (c.total_likes + c.total_dislikes)::float
                ELSE 0.0
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
    let audio_serving = crate::audio_files::serving_path();

    for row in &rows {
        let song_id: i32 = row.get("song_id");
        let generation: i32 = row.get("generation");
        let node: i32 = row.get("node");
        let parent1_id: Option<i32> = row.get("parent1_id");
        let parent2_id: Option<i32> = row.get("parent2_id");
        let likes: i64 = row.get("likes");
        let dislikes: i64 = row.get("dislikes");
        let score: f64 = row.get("score");

        // Copy WAV file to revision
        // First try current generation audio
        let source_wav = audio_serving.join(format!("{}.wav", song_id));
        if source_wav.exists() {
            if let Err(e) = copy_wav_to_revision(revision, song_id, &source_wav) {
                eprintln!("Warning: Failed to copy WAV for song {}: {}", song_id, e);
            }
        } else {
            // Try to find in older generation directories
            let gens_path = crate::audio_files::generations_path();
            if gens_path.exists() {
                let mut found = false;
                if let Ok(entries_dir) = fs::read_dir(&gens_path) {
                    for entry in entries_dir.flatten() {
                        let wav_path = entry.path().join(format!("{}.wav", song_id));
                        if wav_path.exists() {
                            if let Err(e) = copy_wav_to_revision(revision, song_id, &wav_path) {
                                eprintln!("Warning: Failed to copy WAV for song {}: {}", song_id, e);
                            }
                            found = true;
                            break;
                        }
                    }
                }
                if !found {
                    eprintln!("Warning: WAV not found for song {}", song_id);
                }
            }
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

    eprintln!("Updated greatest hits: revision {} with {} songs",
              revision, metadata.songs.len());

    Ok(())
}

/// Check if greatest hits is initialized (has at least one revision).
pub fn is_initialized() -> bool {
    current_symlink_path().exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    // Tests would use a temporary directory
}
