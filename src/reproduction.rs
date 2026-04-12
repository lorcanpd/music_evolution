// src/reproduction.rs

use std::error::Error;
use std::path::Path;
use rand::Rng;
use deadpool_postgres::Pool;

use crate::genome::Genome;
use crate::genome_crosser::GenomeCrosser;
use crate::decode_genome::DecodedGenome;
use crate::play_genes;
use crate::audio_files;

/// Run reproduction with explicit WAV output directory.
/// Returns the number of songs created.
/// Used by the reproduce CLI binary for Slurm jobs.
pub async fn run_reproduction(
    current_generation: i32,
    next_generation: i32,
    pool: &Pool,
    wav_dir: &Path,
) -> Result<usize, Box<dyn Error>> {
    differential_reproduction_impl(current_generation, next_generation, pool, Some(wav_dir)).await
}

/// Steps:
/// 1. Compute total rating for each song in `current_generation_fitness`.
/// 2. Calculate relative fitness within each node.
/// 3. Determine migrations (which node the child goes to).
/// 4. For each node capacity slot, pick parents proportionally to fitness (excluding the same parent).
/// 5. Insert child into `songs` with next_generation, and generate new .wav in current_generation folder.
pub async fn differential_reproduction(
    current_generation: i32,
    next_generation: i32,
    pool: &Pool
) -> Result<(), Box<dyn Error>> {
    differential_reproduction_impl(current_generation, next_generation, pool, None).await?;
    Ok(())
}

/// Internal implementation that supports both caller patterns.
/// If wav_dir is None, uses the default "current_generation" directory.
async fn differential_reproduction_impl(
    current_generation: i32,
    next_generation: i32,
    pool: &Pool,
    wav_dir: Option<&Path>,
) -> Result<usize, Box<dyn Error>> {
    use std::collections::HashMap;

    let client = pool.get().await?;

    // 2. Compute total rating per song
    //    Also retrieve node, so we can compute per-node sums
    let rows = client.query(
        "
        SELECT s.song_id, s.node, SUM(f.rating) + 1 as total_rating
        FROM songs s
        LEFT JOIN current_generation_fitness f ON s.song_id = f.song_id
        WHERE s.generation = $1
        GROUP BY s.song_id
        ",
        &[&current_generation]).await?;

    // Add each songs fitness score to the historic_fitness_scores table
    for row in rows.iter() {
        let song_id: i32 = row.get("song_id");
        let total_rating: i64 = row.get::<_, Option<i64>>("total_rating").unwrap_or(0);
        client.execute(
            "INSERT INTO historic_fitness_scores (song_id, sum_of_ratings) VALUES ($1, $2)",
            &[&song_id, &(total_rating as i32)],
        ).await?;
    }


    // Map: node -> Vec<(song_id, total_rating)>
    let mut node_songs: HashMap<i32, Vec<(i32, i64)>> = HashMap::new();
    for row in rows {
        let song_id: i32 = row.get("song_id");
        let node: i32 = row.get("node");
        let total_rating: Option<i64> = row.get("total_rating");
        let total_rating = total_rating.unwrap_or(0); // handle nulls if no rating

        node_songs.entry(node).or_default().push((song_id, total_rating));
    }

    // 3. Compute per-node sum of ratings and derive relative fitness
    //    We'll store: node -> Vec<(song_id, relative_fitness)>
    let mut node_fitness: HashMap<i32, Vec<(i32, f64)>> = HashMap::new();
    for (&node, songs) in &node_songs {
        let sum_ratings: i64 = songs.iter().map(|(_, r)| *r).sum();
        if sum_ratings == 0 {
            // all zero; fallback to uniform
            let uniform = 1.0 / (songs.len() as f64);
            let fits = songs.iter()
                .map(|(song_id, _)| (*song_id, uniform))
                .collect::<Vec<_>>();
            node_fitness.insert(node, fits);
        } else {
            let fits = songs.iter().map(|(song_id, rating)| {
                let rel = (*rating as f64) / (sum_ratings as f64);
                (*song_id, rel)
            }).collect::<Vec<_>>();
            node_fitness.insert(node, fits);
        }
    }

    // 4. Determine migrations and use that to determine parentage of child slots.
    // For each destination node, create exactly `capacity(destination)` child slots.
    // Each slot samples its source population from the incoming dispersal probabilities,
    // falling back to local reproduction when no migration is selected.
    let habitat_rows = client.query(
        "SELECT node, capacity FROM habitat WHERE node > 0", &[]).await?;
    let mut node_capacities = vec![];
    for row in habitat_rows {
        let node_id: i32 = row.get("node");
        let capacity: i32 = row.get("capacity");
        node_capacities.push((node_id, capacity));
    }

    let dispersal_rows = client.query(
        "SELECT from_node, to_node, probability FROM dispersal_probabilities", &[]).await?;
    // Incoming dispersal probabilities keyed by destination node.
    let mut dispersal_probabilities: HashMap<i32, Vec<(i32, f64)>> = HashMap::new();
    for row in dispersal_rows {
        let from_node: i32 = row.get("from_node");
        let to_node: i32 = row.get("to_node");
        let probability: f64 = row.get("probability");
        let entry = dispersal_probabilities.entry(to_node).or_default();
        entry.push((from_node, probability));
    }

    // Build a plan of (source_node, destination_node), one entry per destination slot.
    let plan = {
        let mut plan = vec![];
        let mut rng = rand::thread_rng();

        for (dest_node, capacity) in &node_capacities {
            let incoming = dispersal_probabilities.get(dest_node);
            for _ in 0..*capacity {
                let source_node = if let Some(incoming) = incoming {
                    let roll = rng.gen_range(0.0..1.0);
                    let mut cumulative = 0.0;
                    let mut selected = *dest_node;

                    for (from_node, probability) in incoming {
                        cumulative += *probability;
                        if roll <= cumulative {
                            selected = *from_node;
                            break;
                        }
                    }

                    selected
                } else {
                    *dest_node
                };

                plan.push((source_node, *dest_node));
            }
        }

        plan
    };

    // 5. Fill each destination slot by sampling parents from the chosen source population.
    // Existing generations may already have inconsistent per-node counts; if a chosen source node
    // has no songs, fall back to the destination node instead of forcing a reset.
    let mut new_songs = vec![];

    for (source_node, dest_node) in plan {
        let fits = node_fitness
            .get(&source_node)
            .or_else(|| node_fitness.get(&dest_node))
            .ok_or_else(|| {
                format!(
                    "No source songs available for reproduction slot: source node {}, destination node {}",
                    source_node, dest_node
                )
            })?;

        // pick two parents
        let (parent1_id, parent2_id) = pick_parents(fits)?;
        // retrieve the actual genome from the DB
        let father_genome: Genome = client.query_one(
            "SELECT genome FROM songs WHERE song_id=$1", &[&parent1_id]
        ).await?.get("genome");
        let mother_genome: Genome = client.query_one(
            "SELECT genome FROM songs WHERE song_id=$1", &[&parent2_id]
        ).await?.get("genome");

        // crossover => child
        let child_genome = GenomeCrosser::crossover(&father_genome, &mother_genome);

        // Insert child
        let row = client.query_one(
            "INSERT INTO songs (generation, node, genome, parent1_id, parent2_id)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING song_id",
            &[
                &next_generation,
                &dest_node,
                &child_genome,
                &parent1_id,
                &parent2_id
            ],
        ).await?;
        let child_id: i32 = row.get(0);

        new_songs.push(child_id);
    }

    // 6. Generate WAV files for the new generation
    //    Use atomic directory swap to avoid "device busy" errors when files are being served

    let song_count = new_songs.len();

    // Determine output directory based on whether we're using explicit path (Slurm) or managed dirs
    let output_dir = if let Some(explicit_path) = wav_dir {
        // Slurm mode: use explicit path, create if needed
        use std::fs;
        if explicit_path.exists() {
            fs::remove_dir_all(explicit_path)?;
        }
        fs::create_dir_all(explicit_path)?;
        explicit_path.to_path_buf()
    } else {
        // Development mode: use generation-numbered directory with atomic swap
        audio_files::init_audio_dirs()?;
        audio_files::create_generation_dir(next_generation)?
    };

    // Generate WAV files for each new song
    for song_id in &new_songs {
        let row = client.query_one(
            "SELECT genome FROM songs WHERE song_id=$1", &[song_id]).await?;
        let genome: Genome = row.get("genome");
        let decoded = DecodedGenome::decode(&genome);

        let filename = output_dir.join(format!("{}.wav", song_id));
        play_genes::generate_wav(&decoded, filename.to_str().unwrap())?;
        eprintln!("Generated WAV: {}", filename.display());
    }

    // Activate the new generation (atomic symlink swap) - only in development mode
    if wav_dir.is_none() {
        audio_files::activate_generation(next_generation)?;

        // Clean up old generations (keep last 2 for safety)
        if let Err(e) = audio_files::cleanup_old_generations(2) {
            eprintln!("Warning: Failed to cleanup old generations: {}", e);
        }
    }

    // Clear out the current_generation_fitness table
    client.execute("DELETE FROM current_generation_fitness", &[]).await?;

    eprintln!("Differential reproduction complete. Next generation = {}, songs created = {}", next_generation, song_count);
    Ok(song_count)
}

/// Weighted random parent selection:
/// pick two distinct parents from `fits: &[(song_id, relative_fitness)]`
fn pick_parents(fits: &[(i32, f64)]) -> Result<(i32, i32), Box<dyn Error>> {
    // pick 1st parent
    let parent1_id = weighted_choice(fits)?;
    // pick 2nd parent from the same list, ignoring parent1
    let mut filtered: Vec<(i32, f64)> = fits.iter().cloned()
        .filter(|(id, _)| *id != parent1_id)
        .collect();

    // re-normalise
    let total: f64 = filtered.iter().map(|(_, w)| w).sum();
    if total > 0.0 {
        for (_, w) in &mut filtered {
            *w /= total;
        }
    } else {
        let filtered_len = filtered.len() as f64;
        // fallback
        for (_, w) in &mut filtered {
            *w = 1.0 / (filtered_len);
        }
    }

    let parent2_id = weighted_choice(&filtered)?;
    Ok((parent1_id, parent2_id))
}

/// Weighted random selection from a slice of (id, fitness).
/// fitness should sum to ~1.0.
fn weighted_choice(fits: &[(i32, f64)]) -> Result<i32, Box<dyn Error>> {
    let mut rng = rand::thread_rng();
    let roll = rng.gen_range(0.0..1.0);

    let mut cumulative = 0.0;
    for (song_id, rel_fit) in fits {
        cumulative += rel_fit;
        if roll <= cumulative {
            return Ok(*song_id);
        }
    }
    // fallback: if rounding errors, pick the last
    Ok(fits.last().unwrap().0)
}
