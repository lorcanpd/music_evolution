// src/user_interaction.rs

use std::io::Write;
use std::error::Error; // (No longer used in route handlers)
use lazy_static::lazy_static;
use rand::Rng;
use rocket::{get, post, State};
use rocket::form::{Form, FromForm};
use rocket::response::{Redirect, content::RawHtml, status};
use maud::{html, Markup};
use deadpool_postgres::Pool;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::io;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
// use music_evo::task_queue::TaskQueue;
use crate::genome::Genome;
use crate::decode_genome::DecodedGenome;
use crate::play_genes::{generate_wav, play_genes, play_precomputed_wav};
use crate::task_queue::{Task, TaskQueue};


// ------------------------------------------
// GLOBAL STATIC to store the "Adam" we last generated
// ------------------------------------------
lazy_static! {
    static ref CURRENT_ADAM: Mutex<Option<Genome>> = Mutex::new(None);
}

pub struct AppState {
    pub pool: Pool,
    pub audio_cache: RwLock<HashMap<i32, Arc<Vec<u8>>>>,
    pub reproduction_in_progress: Arc<AtomicBool>,
    pub first_gen_created: Arc<AtomicBool>,
    pub task_queue: TaskQueue
}


// A small form for user response to Adam (Yes or No)
#[derive(FromForm)]
pub struct AdamResponseForm {
    pub action: String,
}

// A small form for user rating of a song (Yes or No)
#[derive(FromForm)]
pub struct RatingForm {
    pub song_id: i32,
    pub rating: i32,
}

// ------------------------------------------
// GET /choose_adam
// ------------------------------------------
#[get("/choose_adam")]
pub fn get_choose_adam(state: &State<AppState>) -> Result<RawHtml<String>, status::Custom<String>> {
    // 1. Generate random Adam
    let mut adam = Genome::initialise_random_genome(128, 256, 8, 16);
    let mut rng = rand::thread_rng();
    let mutation = rng.gen_range(0.00125..0.07);
    adam.assign_mutation_rate(mutation);

    // 2. Store in the global static
    {
        let mut lock = CURRENT_ADAM.lock().unwrap();
        *lock = Some(adam.clone_genome());
    }

    // 3. Decode and create a temporary WAV file "temp_adam.wav"
    let decoded = DecodedGenome::decode(&adam);
    if let Err(e) = generate_wav(&decoded, "temp_adam.wav") {
        return Err(status::Custom(
            rocket::http::Status::InternalServerError,
            format!("Failed to generate temp_adam.wav: {}", e)
        ));
    }

    // 4. Create the HTML page using maud
    let markup: Markup = html! {
        html {
            head {
                title { "Choose Adam" }
            }
            body {
                h1 { "New Random Adam" }
                p { "Mutation rate assigned. Listen below." }
                audio controls {
                    source src="/temp_adam.wav" type="audio/wav";
                    "Your browser does not support the audio element."
                }
                form action="/choose_adam" method="post" {
                    button type="submit" name="action" value="yes" { "Yes, Accept This Adam" }
                    button type="submit" name="action" value="no" { "Reject, Generate Another" }
                }
            }
        }
    };

    Ok(RawHtml(markup.into_string()))
}

// ------------------------------------------
// POST /choose_adam
// CHANGED: Changed return type from Box<dyn Error> to status::Custom<String>
// ------------------------------------------
#[post("/choose_adam", data="<form_data>")]
pub async fn post_choose_adam(
    state: &State<AppState>, form_data: Form<AdamResponseForm>
) -> Result<Redirect, status::Custom<String>> { // CHANGED: Return type
    let action = &form_data.action;
    if action == "no" {
        return Ok(Redirect::to("/choose_adam"));
    }

    let adam_opt = {
        let lock = CURRENT_ADAM.lock().unwrap();
        lock.clone()
    };
    let mut adam = match adam_opt {
        Some(a) => a,
        None => {
            // CHANGED: Return error as status::Custom<String>
            return Err(status::Custom(
                rocket::http::Status::InternalServerError,
                "No Adam in memory".to_string()
            ));
        }
    };

    // set song_id to 1 for Adam
    adam.assign_song_id(1);

    // CHANGED: Get client from the pool and map errors to status::Custom<String>
    let client = state.pool.get().await.map_err(|e| {
        status::Custom(
            rocket::http::Status::InternalServerError,
            format!("Failed to get DB connection: {}", e)
        )
    })?;

    match client.query_one(
        "INSERT INTO songs (generation, node, genome)
         VALUES ($1, $2, $3)
         RETURNING song_id",
        &[&0, &0, &adam],
    ).await {
        Ok(row) => {
            let song_id: i32 = row.get(0);
            println!("Adam stored in DB with song_id={}", song_id);
        }
        Err(e) => {
            return Err(status::Custom(
                rocket::http::Status::InternalServerError,
                format!("Failed to insert Adam: {}", e)
            ));
        }
    }

    // create eve from adam
    let mut eve = adam.clone_genome();
    eve.assign_song_id(2);

    match client.query_one(
        "INSERT INTO songs (generation, node, genome)
         VALUES ($1, $2, $3)
         RETURNING song_id",
        &[&0, &0, &eve],
    ).await {
        Ok(row) => {
            let song_id: i32 = row.get(0);
            println!("Eve stored in DB with song_id={}", song_id);
        }
        Err(e) => {
            return Err(status::Custom(
                rocket::http::Status::InternalServerError,
                format!("Failed to insert Eve: {}", e)
            ));
        }
    }

    {
        let mut lock = CURRENT_ADAM.lock().unwrap();
        *lock = None;
    }

    Ok(Redirect::to("/create_first_generation"))
}

// ------------------------------------------
// GET /rate_songs
// ------------------------------------------
#[get("/rate_songs")]
pub async fn get_rate_songs(state: &State<AppState>) -> Result<RawHtml<String>, Redirect> {

    if state.reproduction_in_progress.load(Ordering::SeqCst) {
        return Err(Redirect::to("/reproduction_message"));
    }

    let client = state.pool.get().await.map_err(
        |e| Redirect::to(format!("/error?msg={}", e)))?;

    let song_id: i32 = {
        let row = client.query_one(
            "SELECT song_id FROM songs
             WHERE generation = (SELECT MAX(generation) FROM songs)
             ORDER BY RANDOM()
             LIMIT 1",
            &[]
        ).await.map_err(|e| {
            Redirect::to(format!("/error?msg={}", e))
        })?;
        row.get("song_id")
    };

    let markup: Markup = html! {
        html {
            head { title { "Rate a Song" } }
            body {
                h1 { "Rate Song" }
                p { "Listen to the song below and rate it." }
                audio controls {
                    source src=(format!("/song_wav/{}", song_id)) type="audio/wav";
                    "Your browser does not support the audio element."
                }
                form action="/rate_songs" method="post" {
                    input type="hidden" name="song_id" value=(song_id);
                    button type="submit" name="rating" value="1" { "Yes" }
                    button type="submit" name="rating" value="0" { "No" }
                }
            }
        }
    };

    Ok(RawHtml(markup.into_string()))
}

// ------------------------------------------
// POST /rate_songs
// ------------------------------------------
#[post("/rate_songs", data = "<form_data>")]
pub async fn post_rate_songs(
    state: &State<AppState>, form_data: Form<RatingForm>
) -> Result<Redirect, status::Custom<String>> {
    if state.reproduction_in_progress.load(Ordering::SeqCst) {
        return Ok(Redirect::to("/reproduction_message"));
    }
    let song_id = form_data.song_id;
    let rating = form_data.rating;
    // Instead of inserting directly into the DB, enqueue the rating task.
    if let Err(e) = state.task_queue.sender.send(Task::Rating { song_id, rating }).await {
        return Err(status::Custom(
            rocket::http::Status::InternalServerError,
            format!("Failed to enqueue rating task: {}", e)
        ));
    }
    println!("Enqueued rating for song {} with rating {}", song_id, rating);

    Ok(Redirect::to("/rate_songs"))
}

// ------------------------------------------
// Commands for user input through the terminal
// ------------------------------------------
pub fn choose_adam() -> Result<Genome, Box<dyn Error>> {
    loop {
        let mut adam = Genome::initialise_random_genome(128, 256, 8, 16);
        adam.assign_mutation_rate(0.02);

        println!("Accept this Adam? (N/y): ");
        let decoded = DecodedGenome::decode(&adam);
        play_genes(&decoded)?;

        print!("> ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input.is_empty() || input == "n" || input == "no" {
            println!("Generating a new Adam...");
        } else if input == "y" || input == "yes" {
            println!("Adam accepted.");
            return Ok(adam);
        } else {
            println!("Please type 'y' to approve or 'n' to reject.");
        }
    }
}

/// Randomly sample and rate songs until rating_limit ratings are collected.
/// CHANGED: Return type now uses status::Custom<String> for errors.
pub async fn rate_songs(state: &State<AppState>, rating_limit: i32) -> Result<(), status::Custom<String>> {
    let client = state.pool.get().await.map_err(|e| {
        status::Custom(
            rocket::http::Status::InternalServerError,
            format!("Failed to get DB connection: {}", e)
        )
    })?;

    let mut rng = rand::thread_rng();
    let mut ratings_collected = 0;

    println!("Starting rating process. Press 'q' to quit early.");

    while ratings_collected < rating_limit {
        let song_ids_rows = client.query(
            "SELECT song_id FROM songs WHERE generation = (SELECT MAX(generation) FROM songs)",
            &[]
        ).await.map_err(|e| {
            status::Custom(
                rocket::http::Status::InternalServerError,
                format!("Failed to query songs: {}", e)
            )
        })?;

        let song_ids: Vec<i32> = song_ids_rows.iter().map(|row| row.get("song_id")).collect();
        if song_ids.is_empty() {
            println!("No songs in the database. Exiting...");
            break;
        }

        let song_id = song_ids[rng.gen_range(0..song_ids.len())];

        println!("Playing song_id={}", song_id);
        play_precomputed_wav(song_id).map_err(|e| {
            status::Custom(
                rocket::http::Status::InternalServerError,
                format!("Failed to play song: {}", e)
            )
        })?;

        println!("Do you like song {}? N/y or q to quit:", song_id);
        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|e| {
            status::Custom(
                rocket::http::Status::InternalServerError,
                format!("Failed to read input: {}", e)
            )
        })?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("q") {
            println!("Quitting rating early...");
            break;
        }

        let rating: i32 = loop {
            let mut input = String::new();
            io::stdin().read_line(&mut input).map_err(|e| {
                status::Custom(
                    rocket::http::Status::InternalServerError,
                    format!("Failed to read input: {}", e)
                )
            })?;
            let input = input.trim().to_lowercase();

            if input.eq_ignore_ascii_case("q") {
                println!("Quitting rating early...");
                return Ok(());
            }
            match input.as_str() {
                "y" => break 1,
                "" | "n" | _ => break 0,
            };
        };

        client.execute(
            "INSERT INTO current_generation_fitness (song_id, rating)
             VALUES ($1, $2)",
            &[&song_id, &rating],
        ).await.map_err(|e| {
            status::Custom(
                rocket::http::Status::InternalServerError,
                format!("Failed to insert rating: {}", e)
            )
        })?;

        ratings_collected += 1;
        println!("Recorded rating for song {}, total ratings = {}", song_id, ratings_collected);
    }

    Ok(())
}



