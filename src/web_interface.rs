// src/web_interface.rs

use rocket::{get, routes, Route, State};
use rocket::response::Redirect;
use rocket::response::{content::RawHtml};
use rocket::fs::NamedFile;
use deadpool_postgres::Pool;
use crate::task_queue::{Task, TaskQueue};
use crate::initialise_experiment::{create_generation_1, store_current_generation_wavs};
use crate::reproduction::differential_reproduction;
use crate::database::{create_database, populate_habitat_tables};
use crate::audio_files;
use crate::greatest_hits;
use crate::user_interaction::{get_choose_adam, post_choose_adam, get_rate_songs, post_rate_songs, AppState};
use rocket::tokio::sync::broadcast::{self, Sender, error::RecvError};
use rocket::response::stream::{Event, EventStream};
use serde_json::json;
use crate::genome::Genome;
use std::path::{Path, PathBuf};
use tokio::fs;
use std::sync::Arc;
use crate::play_genes::BinaryContent;
use std::sync::atomic::Ordering;
use maud::{html, Markup, PreEscaped, DOCTYPE};

/// Base HTML layout with consistent styling
fn base_layout(title: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" data-theme="dark" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " | Music Evolution" }
                // Pico.css for base styling
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
                // Custom styles
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

/// Serve static files
#[get("/static/<file..>")]
pub async fn static_files(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(file)).await.ok()
}

#[get("/ws")]
pub async fn ws(notify_tx: &State<Sender<()>>) -> EventStream![] {
    let mut rx = notify_tx.subscribe();
    EventStream! {
        loop {
            match rx.recv().await {
                Ok(_) => yield Event::json(&json!("Next generation created")),
                Err(RecvError::Closed) => break,
                Err(RecvError::Lagged(_)) => continue,
            }
        }
    }
}

/// Experiment state for homepage rendering
enum ExperimentState {
    /// Cannot connect to database
    DatabaseError(String),
    /// Tables don't exist yet
    NotInitialized,
    /// Tables exist but no Adam chosen
    NeedsAdam,
    /// Adam chosen but generation 1 not created
    NeedsFirstGeneration,
    /// Fully operational
    Ready {
        current_gen: i32,
        song_count: i64,
        rating_count: i64,
        is_reproducing: bool,
    },
}

/// GET / => Main landing page (never redirects, always shows content).
#[get("/")]
pub async fn index(state: &State<AppState>) -> RawHtml<String> {
    let experiment_state = get_experiment_state(state).await;

    let content = match experiment_state {
        ExperimentState::DatabaseError(msg) => {
            html! {
                article class="card" {
                    h2 { "DATABASE ERROR" }
                    p { "Cannot connect to the database. Please check that PostgreSQL is running." }
                    p class="meta" { (msg) }
                    div class="btn-group" {
                        a href="/" role="button" class="btn btn-primary" { "Retry" }
                    }
                }
            }
        }
        ExperimentState::NotInitialized => {
            html! {
                article class="card" {
                    h2 { "MUSIC EVOLUTION" }
                    p {
                        "Welcome to the evolutionary music experiment. "
                        "Songs evolve based on your ratings. Listen, vote, and guide the evolution of music."
                    }
                    p {
                        "To begin, we need to initialize the experiment and create the primordial song."
                    }
                    div class="btn-group" {
                        a href="/initialise_experiment" role="button" class="btn btn-primary" {
                            "Initialize Experiment"
                        }
                    }
                }
            }
        }
        ExperimentState::NeedsAdam => {
            html! {
                article class="card" {
                    h2 { "CHOOSE THE PRIMORDIAL SONG" }
                    p {
                        "The experiment is initialized. Now you need to choose the first song "
                        "from which all others will evolve."
                    }
                    div class="btn-group" {
                        a href="/choose_adam" role="button" class="btn btn-primary" {
                            "Choose Primordial Song"
                        }
                    }
                }
            }
        }
        ExperimentState::NeedsFirstGeneration => {
            html! {
                article class="card" {
                    h2 { "CREATE FIRST GENERATION" }
                    p {
                        "The primordial song has been chosen. Now we need to create the first generation of songs."
                    }
                    div class="btn-group" {
                        a href="/create_first_generation" role="button" class="btn btn-primary" {
                            "Create First Generation"
                        }
                    }
                }
            }
        }
        ExperimentState::Ready { current_gen, song_count, rating_count, is_reproducing } => {
            html! {
                article class="card" {
                    h2 { "MUSIC EVOLUTION" }
                    p {
                        "Songs evolve based on your ratings. "
                        "Listen and vote on which ones sound good to you. "
                        "The best songs will reproduce to create the next generation."
                    }

                    div class="generation-info" {
                        div class="generation-stat" {
                            span class="value" { (current_gen) }
                            span class="label" { "Generation" }
                        }
                        div class="generation-stat" {
                            span class="value" { (song_count) }
                            span class="label" { "Songs" }
                        }
                        div class="generation-stat" {
                            span class="value" { (rating_count) }
                            span class="label" { "Ratings" }
                        }
                    }

                    @if is_reproducing {
                        div class="message message-info" {
                            span class="status status-running pulse" { "CREATING NEXT GENERATION..." }
                        }
                    }

                    div class="btn-group" {
                        a href="/rate_songs" role="button" class="btn btn-primary" {
                            "Rate Songs"
                        }
                        a href="/greatest_hits" role="button" class="btn btn-secondary" {
                            "Greatest Hits"
                        }
                    }
                }
            }
        }
    };

    RawHtml(base_layout("Home", content).into_string())
}

/// Determine the current state of the experiment.
async fn get_experiment_state(state: &State<AppState>) -> ExperimentState {
    // Try to get database connection
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(e) => return ExperimentState::DatabaseError(e.to_string()),
    };

    // Check if tables exist and have data
    let gen0_count = client
        .query_one("SELECT COUNT(*) as count FROM songs WHERE generation=0", &[])
        .await;

    match gen0_count {
        Err(e) => {
            let err_msg = e.to_string();
            if err_msg.contains("does not exist") || err_msg.contains("relation") {
                ExperimentState::NotInitialized
            } else {
                ExperimentState::DatabaseError(err_msg)
            }
        }
        Ok(row) => {
            let count: i64 = row.get("count");

            // First, verify habitat table is populated (required for foreign keys)
            let habitat_count: i64 = client
                .query_one("SELECT COUNT(*) as count FROM habitat", &[])
                .await
                .map(|r| r.get("count"))
                .unwrap_or(0);

            if habitat_count == 0 {
                // Tables exist but habitat not populated - need full initialization
                return ExperimentState::NotInitialized;
            }

            if count == 0 {
                // Tables exist, habitat populated, but no Adam yet
                ExperimentState::NeedsAdam
            } else if count == 2 {
                // Adam and Eve exist, check if generation 1 exists
                let gen1_count: i64 = client
                    .query_one("SELECT COUNT(*) as count FROM songs WHERE generation=1", &[])
                    .await
                    .map(|r| r.get("count"))
                    .unwrap_or(0);

                if gen1_count == 0 {
                    ExperimentState::NeedsFirstGeneration
                } else {
                    get_ready_state(&client, state).await
                }
            } else {
                get_ready_state(&client, state).await
            }
        }
    }
}

/// Get the ready state with current stats.
async fn get_ready_state(
    client: &deadpool_postgres::Client,
    state: &State<AppState>,
) -> ExperimentState {
    let stats = client
        .query_one(
            "SELECT MAX(generation) as gen, COUNT(*) as songs FROM songs WHERE generation = (SELECT MAX(generation) FROM songs)",
            &[],
        )
        .await
        .ok();

    let (current_gen, song_count) = stats
        .map(|r| (r.get::<_, Option<i32>>("gen").unwrap_or(0), r.get::<_, i64>("songs")))
        .unwrap_or((0, 0));

    let rating_count: i64 = client
        .query_one("SELECT COUNT(*) as count FROM current_generation_fitness", &[])
        .await
        .map(|r| r.get("count"))
        .unwrap_or(0);

    let is_reproducing = state.reproduction_in_progress.load(Ordering::SeqCst);

    ExperimentState::Ready {
        current_gen,
        song_count,
        rating_count,
        is_reproducing,
    }
}

/// GET /initialise_experiment
#[get("/initialise_experiment")]
pub async fn initialise_experiment_route(state: &State<AppState>) -> Result<Redirect, Redirect> {
    create_database(&state.pool).await.map_err(|_| Redirect::to("/error"))?;
    populate_habitat_tables(&state.pool).await.map_err(|_| Redirect::to("/error"))?;
    Ok(Redirect::to("/choose_adam"))
}

#[get("/create_first_generation")]
async fn create_first_generation(state: &State<AppState>) -> Result<Redirect, Redirect> {
    let client = state.pool.get().await.map_err(|_| Redirect::to("/error"))?;
    let adam: Genome = client
        .query_one("SELECT genome FROM songs WHERE generation=0 and song_id=1", &[])
        .await.map_err(|_| Redirect::to("/error"))?
        .get("genome");
    let eve: Genome = client
        .query_one("SELECT genome FROM songs WHERE generation=0 and song_id=2", &[])
        .await.map_err(|_| Redirect::to("/error"))?
        .get("genome");

    drop(client);

    create_generation_1(&state.pool, &adam, &eve)
        .await
        .map_err(|_| Redirect::to("/error"))?;
    store_current_generation_wavs(&state.pool)
        .await
        .map_err(|_| Redirect::to("/error"))?;

    // Update the app state to indicate that the first generation has been created.
    state.first_gen_created.store(true, Ordering::SeqCst);

    Ok(Redirect::to("/"))
}

/// GET /creating_next_generation
#[get("/creating_next_generation")]
pub async fn creating_next_generation_page(
    state: &State<AppState>,
) -> Result<RawHtml<String>, Redirect> {
    if state.production_mode {
        // In production mode, reproduction is handled by external job runner.
        // Display status and poll for updates.
        let content = html! {
            article class="card" {
                h2 { "Evolution Scheduled" }
                div class="spinner" {}
                p style="text-align: center;" {
                    "The next generation will be created by the background job runner."
                }
                div id="status-container" class="message message-info" {
                    p { "Checking status..." }
                }
                p style="text-align: center; color: var(--text-muted);" {
                    "This page will automatically update when a new generation is available."
                }
                div class="btn-group" {
                    a href="/" role="button" class="btn btn-secondary" {
                        "Back to Home"
                    }
                }
            }
            script {
                (PreEscaped(r#"
                    async function checkStatus() {
                        try {
                            const response = await fetch('/reproduction_status');
                            const data = await response.json();
                            const container = document.getElementById('status-container');

                            if (data.status === 'completed') {
                                window.location.href = '/new_generation';
                            } else if (data.status === 'running' || data.status === 'queued') {
                                container.innerHTML = '<p>Status: ' + data.status + '</p>';
                                if (data.job_id) {
                                    container.innerHTML += '<p>Job ID: ' + data.job_id + '</p>';
                                }
                            } else if (data.status === 'failed') {
                                container.className = 'message message-error';
                                container.innerHTML = '<p>Reproduction failed: ' + (data.message || 'Unknown error') + '</p>';
                            } else {
                                container.innerHTML = '<p>Status: ' + (data.status || 'idle') + '</p>';
                            }
                        } catch (e) {
                            console.error('Failed to check status:', e);
                        }
                    }

                    // Check immediately and every 5 seconds
                    checkStatus();
                    setInterval(checkStatus, 5000);
                "#))
            }
        };
        Ok(RawHtml(base_layout("Evolution Scheduled", content).into_string()))
    } else {
        // Development mode: trigger reproduction directly
        state.reproduction_in_progress.store(true, Ordering::SeqCst);

        // Enqueue a reproduction task using the task_queue stored in AppState.
        if let Err(e) = state.task_queue.sender.send(Task::Reproduction).await {
            eprintln!("Failed to enqueue reproduction task: {}", e);
            return Err(Redirect::to("/error"));
        }

        let content = html! {
            article class="card" {
                h2 { "Evolution in Progress" }
                div class="spinner" {}
                p class="pulse" style="text-align: center;" {
                    "The songs are reproducing to create the next generation..."
                }
                div class="progress-container" {
                    div class="progress-bar" style="width: 50%;" {}
                }
                p style="text-align: center; color: var(--text-muted);" {
                    "This page will automatically update when complete."
                }
            }
            script {
                (PreEscaped(r#"
                    const eventSource = new EventSource('/ws');
                    eventSource.onmessage = function(event) {
                        window.location.href = '/new_generation';
                    };
                    eventSource.onerror = function() {
                        // Retry connection after 5 seconds
                        setTimeout(() => window.location.reload(), 5000);
                    };
                "#))
            }
        };
        Ok(RawHtml(base_layout("Creating Next Generation", content).into_string()))
    }
}

/// GET /reproduction_status - JSON endpoint for polling job status
#[get("/reproduction_status")]
pub async fn reproduction_status() -> rocket::response::content::RawJson<String> {
    // Read status from the shared status file
    let status_file = std::env::var("REPRODUCTION_STATUS_FILE")
        .unwrap_or_else(|_| "/srv/shared/jobs/music-evo/current_status.json".to_string());

    match tokio::fs::read_to_string(&status_file).await {
        Ok(content) => {
            // Return the JSON directly
            rocket::response::content::RawJson(content)
        }
        Err(_) => {
            // No status file - return idle status
            rocket::response::content::RawJson(
                r#"{"status": "idle", "message": "No reproduction job scheduled"}"#.to_string()
            )
        }
    }
}

#[get("/new_generation")]
async fn new_generation() -> RawHtml<String> {
    let content = html! {
        article class="card" {
            h2 style="text-align: center;" { "NEW GENERATION" }
            p style="text-align: center;" {
                "The songs have evolved. A new generation of music awaits your judgment."
            }
            div class="btn-group" {
                a href="/rate_songs" role="button" class="btn btn-primary" {
                    "Rate the New Songs"
                }
                a href="/" role="button" class="btn btn-secondary" {
                    "Back to Home"
                }
            }
        }
    };

    RawHtml(base_layout("New Generation", content).into_string())
}

/// GET /temp_adam.wav
#[get("/temp_adam.wav")]
pub async fn get_temp_adam_wav() -> Option<NamedFile> {
    NamedFile::open(Path::new("temp_adam.wav")).await.ok()
}

/// GET /song_wav/<song_id>
#[get("/song_wav/<song_id>")]
pub async fn get_song_wav(song_id: i32, state: &State<AppState>) -> Option<BinaryContent> {
    // First, try to get the data from the LRU cache.
    // Note: LruCache.get() requires &mut self for LRU updates
    {
        let mut cache = state.audio_cache.write().await;
        if let Some(audio) = cache.get(&song_id) {
            return Some(BinaryContent((**audio).clone()));
        }
    }
    // If not cached, load the file asynchronously.
    // Use the audio_files module to get the correct serving path (follows symlink)
    let filename = audio_files::serving_path().join(format!("{}.wav", song_id));
    match fs::read(&filename).await {
        Ok(data) => {
            let arc_data = Arc::new(data.clone());
            let mut cache = state.audio_cache.write().await;
            // LruCache uses put() instead of insert()
            cache.put(song_id, arc_data);
            Some(BinaryContent(data))
        },
        Err(e) => {
            eprintln!("Error loading {}: {}", filename.display(), e);
            None
        }
    }
}

#[get("/reproduction_message")]
pub fn reproduction_message() -> RawHtml<String> {
    let content = html! {
        article class="card" {
            h2 { "Evolution in Progress" }
            div class="spinner" {}
            p style="text-align: center;" {
                "The songs are currently reproducing. Please wait a moment and try again."
            }
            div class="btn-group" {
                a href="/" role="button" class="btn btn-secondary" {
                    "Back to Home"
                }
            }
        }
        script {
            (PreEscaped("setTimeout(() => window.location.href = '/rate_songs', 5000);"))
        }
    };

    RawHtml(base_layout("Reproduction in Progress", content).into_string())
}

/// GET /error
#[get("/error")]
pub fn error_page() -> RawHtml<String> {
    let content = html! {
        article class="card" {
            h2 style="text-align: center;" { "ERROR" }
            p style="text-align: center;" {
                "An error occurred while processing your request. Please try again."
            }
            div class="btn-group" {
                a href="/" role="button" class="btn btn-primary" {
                    "Back to Home"
                }
            }
        }
    };

    RawHtml(base_layout("Error", content).into_string())
}

/// GET /greatest_hits - HTML page showing top rated songs
#[get("/greatest_hits")]
pub async fn greatest_hits_page() -> RawHtml<String> {
    let has_data = greatest_hits::is_initialized();

    let content = if has_data {
        match greatest_hits::load_current_metadata() {
            Ok(metadata) => {
                html! {
                    article class="card" {
                        h2 { "Greatest Hits" }
                        p {
                            "The top " (metadata.songs.len()) " songs across all generations, "
                            "ranked by listener approval."
                        }
                        p class="meta" {
                            "Last updated: Generation " (metadata.trigger_generation)
                        }

                        div class="hits-list" {
                            @for (rank, song) in metadata.songs.iter().enumerate() {
                                div class="hit-entry" {
                                    div class="hit-rank" { "#" (rank + 1) }
                                    div class="hit-details" {
                                        div class="hit-stats" {
                                            span class="hit-score" {
                                                (format!("{:.0}%", song.score * 100.0))
                                            }
                                            span class="hit-votes" {
                                                (song.likes) " / " (song.likes + song.dislikes) " votes"
                                            }
                                        }
                                        div class="hit-meta" {
                                            "Gen " (song.generation) " | Song #" (song.song_id)
                                        }
                                    }
                                    div class="hit-audio" {
                                        audio controls preload="none" {
                                            source src=(format!("/greatest_hits_wav/{}", song.song_id)) type="audio/wav";
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div style="text-align: center; margin-top: 2rem;" {
                        a href="/" class="btn btn-secondary" { "Back to Home" }
                    }
                }
            }
            Err(e) => {
                html! {
                    article class="card" {
                        h2 { "Greatest Hits" }
                        div class="message message-error" {
                            p { "Failed to load greatest hits data: " (e.to_string()) }
                        }
                        div class="btn-group" {
                            a href="/" class="btn btn-secondary" { "Back to Home" }
                        }
                    }
                }
            }
        }
    } else {
        html! {
            article class="card" {
                h2 { "Greatest Hits" }
                p {
                    "No greatest hits data yet. Keep rating songs and the hall of fame "
                    "will be populated after each generation completes."
                }
                div class="btn-group" {
                    a href="/rate_songs" class="btn btn-primary" { "Start Rating" }
                    a href="/" class="btn btn-secondary" { "Back to Home" }
                }
            }
        }
    };

    RawHtml(base_layout("Greatest Hits", content).into_string())
}

/// GET /api/greatest_hits - JSON API endpoint
#[get("/api/greatest_hits")]
pub async fn greatest_hits_api() -> rocket::response::content::RawJson<String> {
    if !greatest_hits::is_initialized() {
        return rocket::response::content::RawJson(
            r#"{"error": "No greatest hits data available"}"#.to_string()
        );
    }

    match greatest_hits::load_current_metadata() {
        Ok(metadata) => {
            match serde_json::to_string(&metadata) {
                Ok(json) => rocket::response::content::RawJson(json),
                Err(e) => rocket::response::content::RawJson(
                    format!(r#"{{"error": "Failed to serialize: {}"}}"#, e)
                ),
            }
        }
        Err(e) => rocket::response::content::RawJson(
            format!(r#"{{"error": "Failed to load metadata: {}"}}"#, e)
        ),
    }
}

/// GET /greatest_hits_wav/<song_id> - serve WAV files from greatest hits archive
#[get("/greatest_hits_wav/<song_id>")]
pub async fn get_greatest_hits_wav(song_id: i32) -> Option<BinaryContent> {
    let filename = greatest_hits::wav_file_path(song_id);
    match fs::read(&filename).await {
        Ok(data) => Some(BinaryContent(data)),
        Err(e) => {
            eprintln!("Error loading greatest hit wav {}: {}", filename.display(), e);
            None
        }
    }
}

/// Combine all routes
pub fn routes() -> Vec<Route> {
    routes![
        index,
        static_files,
        get_temp_adam_wav,
        get_song_wav,
        error_page,
        initialise_experiment_route,
        get_choose_adam,
        post_choose_adam,
        create_first_generation,
        get_rate_songs,
        post_rate_songs,
        creating_next_generation_page,
        new_generation,
        ws,
        reproduction_message,
        reproduction_status,
        greatest_hits_page,
        greatest_hits_api,
        get_greatest_hits_wav
    ]
}
