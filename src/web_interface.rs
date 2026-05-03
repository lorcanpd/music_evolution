// src/web_interface.rs

use rocket::{get, routes, Route, State};
use rocket::response::Redirect;
use rocket::response::{content::RawHtml};
use rocket::fs::NamedFile;
use crate::task_queue::Task;
use crate::initialise_experiment::{create_generation_1, store_current_generation_wavs};
use crate::database::{create_database, populate_habitat_tables};
use crate::audio_files;
use crate::family_trees;
use crate::user_interaction::{get_choose_adam, post_choose_adam, get_rate_songs, post_rate_songs, AppState};
use rocket::tokio::sync::broadcast::{Sender, error::RecvError};
use rocket::response::stream::{Event, EventStream};
use serde_json::json;
use crate::genome::Genome;
use crate::decode_genome::DecodedGenome;
use crate::play_genes;
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
                        p class="support-note" {
                            "If you'd like to support this project, "
                            a href="https://buymeacoffee.com/lorcanpd" target="_blank" rel="noopener noreferrer" {
                                "buy me a coffee"
                            }
                        }
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

/// GET /health - minimal public health check
#[get("/health")]
pub async fn health(state: &State<AppState>) -> (rocket::http::Status, rocket::response::content::RawJson<String>) {
    match state.pool.get().await {
        Ok(client) => match client.query_one("SELECT 1", &[]).await {
            Ok(_) => (
                rocket::http::Status::Ok,
                rocket::response::content::RawJson(r#"{"status":"ok"}"#.to_string()),
            ),
            Err(_) => (
                rocket::http::Status::ServiceUnavailable,
                rocket::response::content::RawJson(r#"{"status":"degraded"}"#.to_string()),
            ),
        },
        Err(_) => (
            rocket::http::Status::ServiceUnavailable,
            rocket::response::content::RawJson(r#"{"status":"degraded"}"#.to_string()),
        ),
    }
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
                        a href="/family_trees" role="button" class="btn btn-secondary" {
                            "Family Trees"
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
    let client = state.pool.get().await.map_err(|e| {
        eprintln!("create_first_generation: Failed to get DB connection: {}", e);
        Redirect::to("/error")
    })?;

    // Query Adam and Eve by generation=0, ordered by song_id (first two songs)
    let rows = client
        .query("SELECT song_id, genome FROM songs WHERE generation=0 ORDER BY song_id LIMIT 2", &[])
        .await
        .map_err(|e| {
            eprintln!("create_first_generation: Failed to query Adam/Eve: {}", e);
            Redirect::to("/error")
        })?;

    if rows.len() < 2 {
        eprintln!("create_first_generation: Expected 2 songs in generation 0, found {}", rows.len());
        return Err(Redirect::to("/error"));
    }

    let adam_id: i32 = rows[0].get("song_id");
    let eve_id: i32 = rows[1].get("song_id");
    let adam: Genome = rows[0].get("genome");
    let eve: Genome = rows[1].get("genome");

    println!("create_first_generation: Adam ID={}, Eve ID={}", adam_id, eve_id);

    drop(client);

    // Create generation 1 in the database
    create_generation_1(&state.pool, &adam, &eve, adam_id, eve_id)
        .await
        .map_err(|e| {
            eprintln!("create_first_generation: Failed to create generation 1: {}", e);
            Redirect::to("/error")
        })?;

    // Mark first generation as created BEFORE WAV generation
    // This allows the song queue to start working even if WAVs fail
    state.first_gen_created.store(true, Ordering::SeqCst);
    println!("create_first_generation: Set first_gen_created=true");

    // Try to generate WAV files - failures are logged but don't block the app
    match store_current_generation_wavs(&state.pool).await {
        Ok(()) => {
            println!("create_first_generation: WAV files created successfully");
        }
        Err(e) => {
            eprintln!("create_first_generation: WARNING - Failed to store WAVs: {}", e);
            eprintln!("create_first_generation: The app will continue but audio playback may fail");
            eprintln!("create_first_generation: Check directory permissions on audio/");
        }
    }

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
            let client = match state.pool.get().await {
                Ok(client) => client,
                Err(db_err) => {
                    eprintln!("Error getting DB client while regenerating song {}: {}", song_id, db_err);
                    return None;
                }
            };

            let row = match client
                .query_opt("SELECT genome FROM songs WHERE song_id = $1", &[&song_id])
                .await
            {
                Ok(Some(row)) => row,
                Ok(None) => {
                    eprintln!("No genome found in DB for missing song {}", song_id);
                    return None;
                }
                Err(db_err) => {
                    eprintln!("Error querying DB for missing song {}: {}", song_id, db_err);
                    return None;
                }
            };

            let genome: Genome = row.get("genome");
            let decoded = DecodedGenome::decode(&genome);
            let data = match play_genes::generate_wav_data(&decoded) {
                Ok(data) => data,
                Err(gen_err) => {
                    eprintln!("Error regenerating WAV for song {} from DB: {}", song_id, gen_err);
                    return None;
                }
            };

            let arc_data = Arc::new(data.clone());
            let mut cache = state.audio_cache.write().await;
            cache.put(song_id, arc_data);
            drop(cache);

            // Best effort: repopulate the current-generation WAV path if it exists.
            if let Some(parent) = filename.parent() {
                if let Err(write_err) = fs::create_dir_all(parent).await {
                    eprintln!(
                        "Error creating parent directory for regenerated WAV {}: {}",
                        filename.display(),
                        write_err
                    );
                } else if let Err(write_err) = fs::write(&filename, &data).await {
                    eprintln!(
                        "Error writing regenerated WAV {}: {}",
                        filename.display(),
                        write_err
                    );
                }
            }

            Some(BinaryContent(data))
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

#[get("/greatest_hits")]
pub fn greatest_hits_redirect() -> Redirect {
    Redirect::to("/family_trees")
}

#[get("/family_trees")]
pub async fn family_trees_page(state: &State<AppState>) -> RawHtml<String> {
    let latest_generation = family_trees::latest_generation(&state.pool).await.unwrap_or(0);
    if latest_generation < 2 {
        let content = html! {
            article class="card family-tree-status" {
                h2 { "Family Trees" }
                p {
                    "Family trees become available once a previous generation exists. "
                    "Create at least two generations to unlock this page."
                }
                div class="btn-group" {
                    a href="/" class="btn btn-secondary" { "Back to Home" }
                }
            }
        };
        return RawHtml(base_layout("Family Trees", content).into_string());
    }

    if family_trees::is_rebuild_in_progress() {
        let content = html! {
            article class="card" {
                h2 { "Family Trees" }
                div class="spinner" {}
                p style="text-align: center;" {
                    "The previous-generation family tree package is being rebuilt."
                }
                div class="btn-group" {
                    a href="/" class="btn btn-secondary" { "Back to Home" }
                }
            }
            script {
                (PreEscaped("setTimeout(() => window.location.reload(), 5000);"))
            }
        };
        return RawHtml(base_layout("Family Trees - Rebuilding", content).into_string());
    }

    if family_trees::needs_rebuild(&state.pool).await.unwrap_or(true) {
        family_trees::ensure_family_trees_background(state.pool.clone());
        let status_msg = family_trees::load_status()
            .map(|status| format!(
                "Status: {}{}",
                status.status,
                status
                    .last_error
                    .map(|error| format!(" ({})", error))
                    .unwrap_or_default()
            ))
            .unwrap_or_else(|| "Status: rebuilding...".to_string());

        let content = html! {
            article class="card" {
                h2 { "Family Trees" }
                div class="spinner" {}
                p style="text-align: center;" {
                    "The family-tree package is missing, stale, or incomplete. A rebuild has been triggered."
                }
                p class="meta" style="text-align: center;" { (status_msg) }
                div class="btn-group" {
                    a href="/family_trees" class="btn btn-primary" { "Retry" }
                    a href="/" class="btn btn-secondary" { "Back to Home" }
                }
            }
            script {
                (PreEscaped("setTimeout(() => window.location.reload(), 5000);"))
            }
        };
        return RawHtml(base_layout("Family Trees - Rebuilding", content).into_string());
    }

    let content = html! {
        section class="family-tree-shell" {
            article class="card family-tree-intro" {
                h2 { "Family Trees" }
                p {
                    "Explore the previous generation, activate one of the three spotlight songs, "
                    "and trace its family back through parents, grandparents, siblings, and cousins."
                }
                p class="meta" {
                    "Edges show direct parent-child relatedness. Hover a node to inspect its similarity to the active spotlight."
                }
            }
            section id="family-tree-app" class="family-tree-app" {
                div class="spinner" {}
            }
        }
        script src="/static/family_trees.js" {}
    };

    RawHtml(base_layout("Family Trees", content).into_string())
}

#[get("/api/family_trees")]
pub async fn family_trees_api(state: &State<AppState>) -> rocket::response::content::RawJson<String> {
    if family_trees::latest_generation(&state.pool).await.unwrap_or(0) < 2 {
        return rocket::response::content::RawJson(
            r#"{"status":"unavailable","message":"Family trees require at least two generations"}"#.to_string()
        );
    }

    if family_trees::is_rebuild_in_progress() {
        return rocket::response::content::RawJson(
            r#"{"status":"rebuilding","message":"Family trees are being rebuilt"}"#.to_string()
        );
    }

    if family_trees::needs_rebuild(&state.pool).await.unwrap_or(true) {
        family_trees::ensure_family_trees_background(state.pool.clone());
        let status_json = family_trees::load_status()
            .and_then(|status| serde_json::to_string(&status).ok())
            .unwrap_or_else(|| r#"{"status":"rebuilding","message":"Rebuild triggered"}"#.to_string());
        return rocket::response::content::RawJson(status_json);
    }

    match family_trees::load_current_metadata() {
        Ok(metadata) => rocket::response::content::RawJson(
            serde_json::to_string(&metadata)
                .unwrap_or_else(|e| format!(r#"{{"status":"error","message":"{}"}}"#, e))
        ),
        Err(error) => {
            family_trees::ensure_family_trees_background(state.pool.clone());
            rocket::response::content::RawJson(
                format!(r#"{{"status":"error","message":"{}"}}"#, error)
            )
        }
    }
}

#[get("/api/family_trees/<spot_index>")]
pub async fn family_tree_spot_api(
    spot_index: usize,
    state: &State<AppState>,
) -> rocket::response::content::RawJson<String> {
    if family_trees::latest_generation(&state.pool).await.unwrap_or(0) < 2 {
        return rocket::response::content::RawJson(
            r#"{"status":"unavailable","message":"Family trees require at least two generations"}"#.to_string()
        );
    }

    if family_trees::needs_rebuild(&state.pool).await.unwrap_or(true) {
        family_trees::ensure_family_trees_background(state.pool.clone());
        return rocket::response::content::RawJson(
            r#"{"status":"rebuilding","message":"Family trees are being rebuilt"}"#.to_string()
        );
    }

    match family_trees::load_spotlight_tree(spot_index) {
        Ok(tree) => rocket::response::content::RawJson(
            serde_json::to_string(&tree)
                .unwrap_or_else(|e| format!(r#"{{"status":"error","message":"{}"}}"#, e))
        ),
        Err(error) => rocket::response::content::RawJson(
            format!(r#"{{"status":"error","message":"{}"}}"#, error)
        ),
    }
}

#[get("/family_tree_wav/<song_id>")]
pub async fn get_family_tree_wav(song_id: i32, state: &State<AppState>) -> Option<BinaryContent> {
    {
        let mut cache = state.audio_cache.write().await;
        if let Some(audio) = cache.get(&song_id) {
            return Some(BinaryContent((**audio).clone()));
        }
    }

    let filename = family_trees::wav_file_path(song_id);
    match fs::read(&filename).await {
        Ok(data) => {
            let arc_data = Arc::new(data.clone());
            let mut cache = state.audio_cache.write().await;
            cache.put(song_id, arc_data);
            Some(BinaryContent(data))
        }
        Err(error) => {
            eprintln!("Error loading family tree wav {}: {}", filename.display(), error);

            let client = match state.pool.get().await {
                Ok(client) => client,
                Err(db_error) => {
                    eprintln!("Error getting DB client for family tree wav {}: {}", song_id, db_error);
                    return None;
                }
            };

            let row = match client
                .query_opt("SELECT genome FROM songs WHERE song_id = $1", &[&song_id])
                .await
            {
                Ok(Some(row)) => row,
                Ok(None) => return None,
                Err(db_error) => {
                    eprintln!("Error querying DB for family tree wav {}: {}", song_id, db_error);
                    return None;
                }
            };

            let genome: Genome = row.get("genome");
            let decoded = DecodedGenome::decode(&genome);
            let data = match play_genes::generate_wav_data(&decoded) {
                Ok(data) => data,
                Err(gen_error) => {
                    eprintln!("Error regenerating family tree wav {}: {}", song_id, gen_error);
                    return None;
                }
            };

            let arc_data = Arc::new(data.clone());
            let mut cache = state.audio_cache.write().await;
            cache.put(song_id, arc_data);
            drop(cache);

            if let Some(parent) = filename.parent() {
                if let Err(dir_error) = fs::create_dir_all(parent).await {
                    eprintln!("Error creating family tree audio dir {}: {}", parent.display(), dir_error);
                } else if let Err(write_error) = fs::write(&filename, &data).await {
                    eprintln!("Error writing family tree wav {}: {}", filename.display(), write_error);
                }
            }

            Some(BinaryContent(data))
        }
    }
}

/// Combine all routes
pub fn routes() -> Vec<Route> {
    routes![
        index,
        health,
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
        greatest_hits_redirect,
        family_trees_page,
        family_trees_api,
        family_tree_spot_api,
        get_family_tree_wav
    ]
}
