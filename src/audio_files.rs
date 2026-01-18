// src/audio_files.rs
//
// Manages audio file storage with atomic generation switching.
// Uses generation-numbered directories with a symlink for zero-downtime updates.
//
// Directory structure:
//   audio/
//   ├── generations/
//   │   ├── 1/
//   │   │   ├── 3.wav, 4.wav, ...
//   │   └── 2/
//   │       ├── 47.wav, 48.wav, ...
//   └── current -> generations/2  (symlink)

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Base directory for all audio files
pub const AUDIO_BASE_DIR: &str = "audio";

/// Subdirectory containing numbered generation folders
pub const GENERATIONS_SUBDIR: &str = "generations";

/// Name of the symlink pointing to the current generation
pub const CURRENT_SYMLINK_NAME: &str = "current";

/// Get the path to the audio base directory
pub fn audio_base_path() -> PathBuf {
    PathBuf::from(AUDIO_BASE_DIR)
}

/// Get the path to the generations directory
pub fn generations_path() -> PathBuf {
    audio_base_path().join(GENERATIONS_SUBDIR)
}

/// Get the path to the current symlink
pub fn current_symlink_path() -> PathBuf {
    audio_base_path().join(CURRENT_SYMLINK_NAME)
}

/// Get the path to a specific generation's directory
pub fn generation_path(generation: i32) -> PathBuf {
    generations_path().join(generation.to_string())
}

/// Get the path where WAV files should be served from (follows symlink)
pub fn serving_path() -> PathBuf {
    current_symlink_path()
}

/// Initialize the audio directory structure.
/// Creates the base directories if they don't exist.
pub fn init_audio_dirs() -> io::Result<()> {
    fs::create_dir_all(generations_path())?;
    Ok(())
}

/// Create a directory for a new generation.
/// Returns the path to the new directory.
pub fn create_generation_dir(generation: i32) -> io::Result<PathBuf> {
    let gen_path = generation_path(generation);

    // Remove if exists (shouldn't happen in normal operation)
    if gen_path.exists() {
        fs::remove_dir_all(&gen_path)?;
    }

    fs::create_dir_all(&gen_path)?;
    Ok(gen_path)
}

/// Atomically activate a generation by updating the symlink.
/// This is the key operation that makes the switch safe for concurrent readers.
///
/// The process:
/// 1. Create a temporary symlink pointing to the new generation
/// 2. Atomically rename the temp symlink to replace the current one
///
/// On POSIX systems, rename() is atomic when both paths are on the same filesystem.
pub fn activate_generation(generation: i32) -> Result<(), Box<dyn Error>> {
    let target = generation_path(generation);
    let symlink = current_symlink_path();
    let temp_symlink = audio_base_path().join(".current_new");

    // Verify the target exists
    if !target.exists() {
        return Err(format!("Generation directory does not exist: {}", target.display()).into());
    }

    // Remove temp symlink if it exists from a previous failed attempt
    let _ = fs::remove_file(&temp_symlink);

    // Create relative symlink target (e.g., "generations/2" not absolute path)
    // This makes the symlink portable across different mount points
    let relative_target = PathBuf::from(GENERATIONS_SUBDIR).join(generation.to_string());

    // Create the new symlink at a temporary location
    #[cfg(unix)]
    std::os::unix::fs::symlink(&relative_target, &temp_symlink)?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&relative_target, &temp_symlink)?;

    // Atomically replace the current symlink
    // On Unix, rename() is atomic when src and dst are on the same filesystem
    fs::rename(&temp_symlink, &symlink)?;

    eprintln!("Activated generation {} (symlink: {} -> {})",
              generation, symlink.display(), relative_target.display());

    Ok(())
}

/// Clean up old generation directories, keeping only the most recent N generations.
/// Errors are logged but not propagated - cleanup is best-effort.
pub fn cleanup_old_generations(keep_count: usize) -> Result<(), Box<dyn Error>> {
    let gens_path = generations_path();

    if !gens_path.exists() {
        return Ok(());
    }

    // Collect all generation directories
    let mut generations: Vec<i32> = Vec::new();
    for entry in fs::read_dir(&gens_path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                if let Ok(gen) = name.parse::<i32>() {
                    generations.push(gen);
                }
            }
        }
    }

    // Sort descending (newest first)
    generations.sort_by(|a, b| b.cmp(a));

    // Remove old generations beyond keep_count
    for gen in generations.iter().skip(keep_count) {
        let gen_path = generation_path(*gen);
        match fs::remove_dir_all(&gen_path) {
            Ok(_) => eprintln!("Cleaned up old generation directory: {}", gen_path.display()),
            Err(e) => {
                // Log but don't fail - the directory might be busy
                eprintln!("Warning: Failed to clean up {}: {} (will retry later)",
                         gen_path.display(), e);
            }
        }
    }

    Ok(())
}

/// Get the path to a specific WAV file in the current generation.
pub fn wav_file_path(song_id: i32) -> PathBuf {
    serving_path().join(format!("{}.wav", song_id))
}

/// Check if the audio directory structure is initialized.
pub fn is_initialized() -> bool {
    generations_path().exists()
}

/// Check if a generation is currently active (symlink exists and points to it).
pub fn is_generation_active(generation: i32) -> bool {
    let symlink = current_symlink_path();
    if !symlink.exists() {
        return false;
    }

    match fs::read_link(&symlink) {
        Ok(target) => {
            let expected = PathBuf::from(GENERATIONS_SUBDIR).join(generation.to_string());
            target == expected
        }
        Err(_) => false,
    }
}

/// Scrub all audio directories (used when resetting the experiment).
/// More aggressive than cleanup - removes everything.
pub fn scrub_audio_dirs() -> io::Result<()> {
    let base = audio_base_path();
    if base.exists() {
        // First remove the symlink to avoid "directory not empty" errors
        let symlink = current_symlink_path();
        if symlink.exists() || symlink.is_symlink() {
            fs::remove_file(&symlink)?;
        }

        // Then remove the generations directory
        let gens = generations_path();
        if gens.exists() {
            fs::remove_dir_all(&gens)?;
        }
    }

    // Recreate the structure
    init_audio_dirs()?;

    eprintln!("Scrubbed audio directories");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    // Note: Tests would use a temporary directory to avoid affecting real data
}
