// src/user_interaction.rs

use std::io::Write;
use std::error::Error; // (No longer used in route handlers)
use lazy_static::lazy_static;
use rand::Rng;
use rocket::{get, post, State};
use rocket::form::{Form, FromForm};
use rocket::response::{Redirect, content::RawHtml, status};
use maud::{html, Markup, PreEscaped, DOCTYPE};
use deadpool_postgres::Pool;
use lru::LruCache;
use std::num::NonZeroUsize;

/// Maximum number of audio files to cache in memory
pub const AUDIO_CACHE_MAX_ENTRIES: usize = 100;

/// Base HTML layout with consistent styling
fn base_layout(title: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" data-theme="dark" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " | Music Evolution" }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
                link rel="stylesheet" href="/static/style.css";
            }
            body {
                main class="container" {
                    header {
                        h1 { "Music Evolution" }
                        p class="subtitle" { "From beeps and boops, to beats and bops" }
                    }
                    (content)
                    footer {
                        p { "An evolutionary music experiment" }
                    }
                }
            }
        }
    }
}
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::io;

use tokio::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
// use music_evo::task_queue::TaskQueue;
use crate::genome::Genome;
use crate::decode_genome::DecodedGenome;
use crate::play_genes::{generate_wav, play_genes, play_precomputed_wav};
use crate::task_queue::{Task, TaskQueue};
use crate::song_queue::SongQueue;
use tokio::sync::mpsc::Receiver;


// ------------------------------------------
// GLOBAL STATIC to store the "Adam" we last generated
// ------------------------------------------
lazy_static! {
    static ref CURRENT_ADAM: Arc<Mutex<Option<Genome>>> = Arc::new(Mutex::new(None::<Genome>));
}

pub struct AppState {
    pub pool: Pool,
    /// LRU-bounded audio cache (max AUDIO_CACHE_MAX_ENTRIES entries)
    /// Wrapped in Arc to allow sharing with task queue worker for cache clearing
    pub audio_cache: Arc<RwLock<LruCache<i32, Arc<Vec<u8>>>>>,
    pub reproduction_in_progress: Arc<AtomicBool>,
    pub first_gen_created: Arc<AtomicBool>,
    pub task_queue: TaskQueue,
    pub song_queue_sender: SongQueue,
    pub song_queue_receiver: Arc<Mutex<Receiver<i32>>>,
    /// Production mode: reproduction is handled by external job runner
    pub production_mode: bool,
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
pub async fn get_choose_adam(state: &State<AppState>) -> Result<RawHtml<String>, status::Custom<String>> {
    // 1. Generate random Adam
    let mut adam = Genome::initialise_random_genome(
        128, 256, 8, 16
    );
    let mutation = {
        let mut rng = rand::thread_rng();
        rng.gen_range(0.00125..0.07)
    };
    adam.assign_mutation_rate(mutation);

    // 2. Store in the global static
    {
        let mut lock = CURRENT_ADAM.lock().await;
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

    // 4. Create the HTML page using maud with proper styling
    let content = html! {
        article class="card" {
            h2 { "Choose the Primordial Song" }
            p {
                "This is the first song from which all others will evolve. "
                "Listen carefully and decide if this is a worthy ancestor."
            }

            div class="audio-container" {
                div class="waveform" {}
                audio controls autoplay {
                    source src="/temp_adam.wav" type="audio/wav";
                    "Your browser does not support the audio element."
                }
            }

            form action="/choose_adam" method="post" {
                div class="btn-group" {
                    button type="submit" name="action" value="yes" class="btn btn-primary" {
                        "Accept This Song"
                    }
                    button type="submit" name="action" value="no" class="btn btn-secondary" {
                        "Generate Another"
                    }
                }
            }
        }
    };

    Ok(RawHtml(base_layout("Choose Adam", content).into_string()))
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
        let lock = CURRENT_ADAM.lock().await;
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
        let mut lock = CURRENT_ADAM.lock().await;
        // let mut lock = CURRENT_ADAM.lock().await;
        *lock = None;
    }

    Ok(Redirect::to("/create_first_generation"))
}

// ------------------------------------------
// GET /rate_songs
// ------------------------------------------
#[get("/rate_songs")]
pub async fn get_rate_songs(state: &State<AppState>) -> Result<RawHtml<String>, Redirect> {
    use tokio::time::{timeout, Duration};

    if state.reproduction_in_progress.load(Ordering::SeqCst) {
        return Err(Redirect::to("/reproduction_message"));
    }

    // Check if first generation exists - if not, redirect to home
    if !state.first_gen_created.load(Ordering::SeqCst) {
        eprintln!("get_rate_songs: first_gen_created is false, redirecting to home");
        return Err(Redirect::to("/"));
    }

    // Await a song ID from the pre‑fetched song queue with a timeout
    let song_id = {
        let mut rx = state.song_queue_receiver.lock().await;

        // Wait up to 10 seconds for a song - this gives the queue time to populate
        match timeout(Duration::from_secs(10), rx.recv()).await {
            Ok(Some(song)) => {
                // Notify the song queue that one song was consumed.
                state.song_queue_sender.song_consumed();
                song
            }
            Ok(None) => {
                eprintln!("get_rate_songs: Song queue closed unexpectedly");
                return Err(Redirect::to("/error"));
            }
            Err(_) => {
                eprintln!("get_rate_songs: Timeout waiting for song from queue");
                // Queue might be empty or stuck - try to redirect gracefully
                return Err(Redirect::to("/"));
            }
        }
    };

    let content = html! {
        article class="card" {
            h2 { "Rate This Song" }
            p {
                "Listen to the song and vote. Does it sound good to you? "
                "Your rating helps guide the evolution of the music."
            }

            div class="audio-container" {
                div class="waveform" {}
                audio controls autoplay id="song-player" {
                    source src=(format!("/song_wav/{}", song_id)) type="audio/wav";
                    "Your browser does not support the audio element."
                }
            }

            form action="/rate_songs" method="post" id="rating-form" {
                input type="hidden" name="song_id" value=(song_id);
                div class="btn-group" {
                    button type="submit" name="rating" value="1" class="btn btn-primary" {
                        "I Like It"
                    }
                    button type="submit" name="rating" value="0" class="btn btn-secondary" {
                        "Not For Me"
                    }
                }
            }

            div style="text-align: center; margin-top: 1rem;" {
                a href="/" style="color: var(--text-muted);" { "Back to Home" }
            }
        }
    };

    Ok(RawHtml(base_layout("Rate Songs", content).into_string()))
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
    // Enqueue the rating task instead of directly updating the DB.
    state.task_queue.sender.send(Task::Rating {
        song_id: form_data.song_id,
        rating: form_data.rating,
    }).await.map_err(|e| {
        status::Custom(rocket::http::Status::InternalServerError,
                       format!("Failed to enqueue rating task: {}", e))
    })?;
    println!("Enqueued rating for song {} with rating {}", form_data.song_id, form_data.rating);
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



