// src/reproduction.rs
use std::error::Error;
use rand::Rng;
// use postgres::{Client, NoTls};
use tokio_postgres::{Client, NoTls, connect};

use crate::genome::Genome;
use crate::genome_crosser::GenomeCrosser;
use crate::decode_genome::DecodedGenome;
use crate::play_genes; // for generate_wav

/// Steps:
/// 1. Compute total rating for each song in `current_generation_fitness`.
/// 2. Calculate relative fitness within each node.
/// 3. Determine migrations (which node the child goes to).
/// 4. For each node capacity slot, pick parents proportionally to fitness (excluding the same parent).
/// 5. Insert child into `songs` with next_generation, and generate new .wav in current_generation folder.
pub async fn differential_reproduction(
    current_generation: i32,
    next_generation: i32,
) -> Result<(), Box<dyn Error>> {

    // 1. Connect to DB
    let database_url = std::env::var("DATABASE_URL")?;
    let (mut client, connection) = connect(&database_url, NoTls).await?;

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
        let total_rating: i64 = row.get("total_rating");
        client.execute(
            "INSERT INTO historic_fitness_scores (song_id, sum_of_ratings) VALUES ($1, $2)",
            &[&song_id, &total_rating],
        ).await?;
    }

    // Map: node -> Vec<(song_id, total_rating)>
    use std::collections::HashMap;
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
    // For each node, use the to_node probabilities to determine which parents slots are to be
    // assigned to other nodes.
    let habitat_rows = client.query(
        "SELECT node, capacity FROM habitat", &[]).await?;
    let mut node_capacities = vec![];
    for row in habitat_rows {
        let node_id: i32 = row.get("node");
        let capacity: i32 = row.get("capacity");
        node_capacities.push((node_id, capacity));
    }

    let dispersal_rows = client.query(
        "SELECT from_node, to_node, probability FROM dispersal_probabilities", &[]).await?;
    // get the total capacity of each node and then determine which of these are to be populated
    // for another node.
    // Dispersal probabilities are a hash map of hash maps, where the key is the from_node and the
    // value is a veector of all the to_nodes and their probability tuple pairs.
    let mut dispersal_probabilities: HashMap<i32, Vec<(i32, f64)>> = HashMap::new();
    // let mut dispersal_probabilities: HashMap<i32, HashMap<i32, f64>> = HashMap::new();
    for row in dispersal_rows {
        let from_node: i32 = row.get("from_node");
        let to_node: i32 = row.get("to_node");
        let probability: f64 = row.get("probability");
        // need the data in a hash map that uses the to_node as the key, returning a vector
        // containing the tuples of from_node and probability
        let entry = dispersal_probabilities.entry(to_node).or_default();
        entry.push((from_node, probability));
    }

    // Then for each node we iterate through the nodes to create a reproduction plan of
    // (parent_node, child_node) for every slot. By default, the child_node is the same as the
    // parent_node, unless the child_node is to be populated by another node.
    // let mut reproduction_plan: HashMap<i32, Vec<(i32, i32)>> = HashMap::new();
    let mut plan = vec![];
    for (node, _) in node_capacities.clone() {
        let dispersal = dispersal_probabilities.get(&node);
        if let Some(dispersal) = dispersal {
            // if there are dispersal probabilities for this node, we need to determine which
            // slots are to be populated by other nodes.
            let mut rng = rand::thread_rng();
            // loop through the dispersal from_node and probability pairs in the dispersal vector.
            // if the random number is less than the probability, then the slot is to be populated
            // by the from_node.
            for (from_node, probability) in dispersal {
                let roll = rng.gen_range(0.0..1.0);
                if roll <= *probability {
                    plan.push((*from_node, node));
                } else {
                    plan.push((node, node));
                }
            }
        }
    }

    // 5. For each node, fill 'capacity' child slots from that same node
    //    using WeightedChoice on 'node_fitness[node]' to pick parents
    //    Insert new songs into DB with generation=next_generation
    //    Then create .wav files

    // We'll store newly created songs in order to generate wav files
    let mut new_songs = vec![];

    for (node, dest_node) in plan {

        let fits = node_fitness.get(&node).unwrap();

        let capacity = node_capacities.iter().find(|(id, _)| *id == node).unwrap().1;
        for _ in 0..capacity {
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
    }

    // 6. Overwrite the current_generation folder with newly created songs
    // remove old, create new, or just empty it
    use std::fs;
    if std::path::Path::new("current_generation").exists() {
        fs::remove_dir_all("current_generation")?;
    }
    fs::create_dir_all("current_generation")?;

    // for each new song, decode => generate wav
    // or decode from the child's genome if you want
    for song_id in new_songs {
        let row = client.query_one(
            "SELECT genome FROM songs WHERE song_id=$1", &[&song_id]).await?;
        let genome: Genome = row.get("genome");
        let decoded = DecodedGenome::decode(&genome);

        let filename = format!("current_generation/{}.wav", song_id);
        play_genes::generate_wav(&decoded, &filename)?;
    }

    // clear out the current_generation_fitness table
    client.execute("DELETE FROM current_generation_fitness", &[]).await?;

    println!("Differential reproduction complete. Next generation = {}", next_generation);
    Ok(())
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

