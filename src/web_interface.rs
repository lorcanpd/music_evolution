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
use crate::user_interaction::{get_choose_adam, post_choose_adam, get_rate_songs, post_rate_songs, AppState};
use rocket::tokio::sync::broadcast::{self, Sender, error::RecvError};
use rocket::response::stream::{Event, EventStream};
use serde_json::json;
use crate::genome::Genome;
use std::path::Path;
use tokio::fs;
use std::sync::Arc;
use crate::play_genes::BinaryContent;
use std::sync::atomic::Ordering;

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

/// GET / => Main landing page.
#[get("/")]
pub async fn index(state: &State<AppState>) -> Result<RawHtml<String>, Redirect> {
    let client = state.pool.get().await.map_err(|_| Redirect::to("/error"))?;
    let row = client
        .query_one("SELECT COUNT(*) as count FROM songs WHERE generation=0", &[])
        .await;
    match row {
        Ok(r) => {
            let count: i64 = r.get("count");
            if count == 0 {
                Err(Redirect::to("/initialise_experiment"))
            } else {
                let html = format!(r#"
                    <html>
                      <head><title>From beeps and boops, to beats and bops</title></head>
                      <body>
                        <h1>Welcome to the Experiment</h1>
                        <p>You can <a href="/rate_songs">rate songs</a> to create the selection gradient.</p>
                      </body>
                    </html>
                "#);
                Ok(RawHtml(html))
            }
        }
        Err(_) => Err(Redirect::to("/error")),
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
) -> Result<RawHtml<&'static str>, Redirect> {
    // Set the reproduction flag.
    state.reproduction_in_progress.store(true, Ordering::SeqCst);

    // Enqueue a reproduction task using the task_queue stored in AppState.
    if let Err(e) = state.task_queue.sender.send(Task::Reproduction).await {
        eprintln!("Failed to enqueue reproduction task: {}", e);
        return Err(Redirect::to("/error"));
    }

    Ok(RawHtml(r#"
        <h1>Reproduction in progress</h1>
        <p>The reproduction task has been enqueued. Please wait while the new generation is created.</p>
        <script>
            const eventSource = new EventSource('/ws');
            eventSource.onmessage = function(event) {
                alert(event.data);
                window.location.href = '/new_generation';
            };
        </script>
    "#))
}

#[get("/new_generation")]
async fn new_generation() -> RawHtml<&'static str> {
    RawHtml("<h1>New generation created</h1><p><a href='/rate_songs'>Rate the new songs</a></p>")
}

/// GET /temp_adam.wav
#[get("/temp_adam.wav")]
pub async fn get_temp_adam_wav() -> Option<NamedFile> {
    NamedFile::open(Path::new("temp_adam.wav")).await.ok()
}

/// GET /song_wav/<song_id>
#[get("/song_wav/<song_id>")]
pub async fn get_song_wav(song_id: i32, state: &State<AppState>) -> Option<BinaryContent> {
    // First, try to get the data from the in-memory cache.
    {
        let cache = state.audio_cache.read().await;
        if let Some(audio) = cache.get(&song_id) {
            return Some(BinaryContent((**audio).clone()));
        }
    }
    // If not cached, load the file asynchronously.
    let filename = format!("current_generation/{}.wav", song_id);
    match fs::read(&filename).await {
        Ok(data) => {
            let arc_data = Arc::new(data.clone());
            let mut cache = state.audio_cache.write().await;
            cache.insert(song_id, arc_data);
            Some(BinaryContent(data))
        },
        Err(e) => {
            eprintln!("Error loading {}: {}", filename, e);
            None
        }
    }
}

#[get("/reproduction_message")]
pub fn reproduction_message() -> RawHtml<&'static str> {
    RawHtml("<h1>Reproduction in progress</h1><p>Please wait while the new generation is being created.</p>")
}

/// GET /error
#[get("/error")]
pub fn error_page() -> RawHtml<&'static str> {
    RawHtml("<h1>Something went wrong</h1>")
}

/// Combine all routes
pub fn routes() -> Vec<Route> {
    routes![
        index,
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
        reproduction_message
    ]
}
