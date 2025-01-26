// src/web_interface.rs

use dotenv::dotenv;
use rocket::{get, routes, Route, State};
use rocket::fs::NamedFile;
use rocket::response::{content::RawHtml, Redirect};
use rocket::http::Status;
use std::path::Path;
use std::io;
use std::error::Error;
use std::sync::Mutex;

use tokio_postgres::{connect, Client as AsyncClient, NoTls};
use crate::genome::Genome;
use crate::initialise_experiment::{
    scrub_database, create_generation_1,
    store_current_generation_wavs
};
use crate::reproduction::differential_reproduction;

use crate::database::{create_database, populate_habitat_tables};

use crate::user_interaction::{
    get_choose_adam,
    post_choose_adam,
    get_rate_songs,
    post_rate_songs,
    AppState,
};

use rocket::tokio::sync::broadcast::{self, Sender, error::RecvError};
use rocket::response::stream::{Event, EventStream};
// use rocket::serde::json::Json;
// use rocket::State;
use serde_json::json;


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

/// GET / => Main landing page. Checks if experiment is initialised:
///  - If no Adam in the DB => redirect to /choose_adam
///  - Otherwise => link to /rate_songs
#[get("/")]
pub async fn index(state: &State<AppState>) -> Result<RawHtml<String>, Redirect> {
    // Get the asynchronous client from the state
    let client = &state.client;

    // Check if there's at least one song with generation=0, i.e. Adam
    let row = client.query_one(
        "SELECT COUNT(*) as count FROM songs WHERE generation=0",
        &[]
    ).await;
    match row {
        Ok(r) => {
            let count: i64 = r.get("count");
            if count == 0 {
                // no songs with generation=0 => redirect to /initialise_experiment
                Err(Redirect::to("/initialise_experiment"))
            } else {
                // Adam present => show a landing with a link to /rate_songs
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
        Err(_) => {
            // Could not query => redirect or show error
            Err(Redirect::to("/error"))
        }
    }
}

/// GET /initialise_experiment => Initialise the experiment:
/// - Create the database tables
/// - Populate the habitat table
/// - Create Adam and Eve
/// - Create the first generation
/// - Store the .wav files for the first generation
/// - Redirect to /rate_songs
/// This is the first page the user sees when the experiment is not initialised.
/// This is a long-running operation, so it is done in a separate route.
/// This route is not part of the main routes, but is called from the main routes.

#[get("/initialise_experiment")]
pub async fn initialise_experiment_route(state: &State<AppState>) -> Result<Redirect, Redirect> {
    // Get the asynchronous client from the state
    let client = &state.client;

    // Create the database tables
    create_database().await.map_err(|_| Redirect::to("/error"))?;

    // Populate the habitat table
    populate_habitat_tables().await.map_err(|_| Redirect::to("/error"))?;

    // Redirect to /rate_songs
    Ok(Redirect::to("/choose_adam"))
}

#[get("/create_first_generation")]
async fn create_first_generation(state: &State<AppState>) -> Result<Redirect, Redirect> {
    // Get the asynchronous client from the state
    let client = &state.client;

    // Get adam from database
    let adam: Genome = client.query_one(
        "SELECT genome FROM songs WHERE generation=0 and song_id=1",
        &[]
    ).await.map_err(|_| Redirect::to("/error"))?.get("genome");

    // Get eve from database
    let eve: Genome = client.query_one(
        "SELECT genome FROM songs WHERE generation=0 and song_id=2",
        &[]
    ).await.map_err(|_| Redirect::to("/error"))?.get("genome");

    // Create the first generation
    create_generation_1(client, &adam, &eve).await.map_err(|_| Redirect::to("/error"))?;

    // Store the .wav files for the first generation
    store_current_generation_wavs(client).await.map_err(|_| Redirect::to("/error"))?;

    // Redirect to /
    Ok(Redirect::to("/"))
}

#[get("/creating_next_generation")]
pub async fn creating_next_generation_page(state: &State<AppState>, notify_tx: &State<Sender<()>>) -> Result<RawHtml<&'static str>, Redirect> {
    dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").map_err(|_| Redirect::to("/error"))?;
    let (client, connection) = connect(&database_url, NoTls).await.map_err(|_| Redirect::to("/error"))?;

    let notify_tx_clone = notify_tx.inner().clone();

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    tokio::spawn(async move {
        match client.query_one("SELECT MAX(generation) as curr_gen FROM songs", &[]).await {
            Ok(row) => {
                let current_generation: i32 = row.get("curr_gen");
                println!("Current generation: {}", current_generation);

                match differential_reproduction(current_generation, current_generation + 1).await {
                    Ok(_) => {
                        println!("Differential reproduction succeeded");
                        let _ = notify_tx_clone.send(());
                    }
                    Err(e) => {
                        eprintln!("Differential reproduction error: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Query error: {}", e);
            }
        }
    });

    Ok(RawHtml(r#"
        <h1>Creating the next generation...</h1>
        <p>Please wait while the database is being updated.</p>
        <script>
            const eventSource = new EventSource('/ws');
            eventSource.onmessage = function(event) {
                alert(event.data);
                window.location.href = '/new_generation';
            };
        </script>
    "#))
}

// New generation just tells the user that the new generation has been created, and links them to /rate_songs
#[get("/new_generation")]
async fn new_generation() -> RawHtml<&'static str> {
    RawHtml("<h1>New generation created</h1><p><a href='/rate_songs'>Rate the new songs</a></p>")
}

/// GET /temp_adam.wav => serve the file "temp_adam.wav" from disk
/// This is used by the user_interaction code to let user listen to the random Adam.
#[get("/temp_adam.wav")]
pub async fn get_temp_adam_wav() -> Option<NamedFile> {
    NamedFile::open(Path::new("temp_adam.wav")).await.ok()
}

/// GET /song_wav/<id> => serve the .wav file for a particular song from current_generation/
#[get("/song_wav/<song_id>")]
pub async fn get_song_wav(song_id: i32) -> Option<NamedFile> {
    let filename = format!("current_generation/{}.wav", song_id);
    NamedFile::open(Path::new(&filename)).await.ok()
}

/// GET /error => a simple error page
#[get("/error")]
pub fn error_page() -> RawHtml<&'static str> {
    RawHtml("<h1>Something went wrong</h1>")
}


/// Combine all routes in one function
pub fn routes() -> Vec<Route> {
    routes![
        index,
        get_temp_adam_wav,
        get_song_wav,
        error_page,
        // from user_interaction:
        initialise_experiment_route,
        get_choose_adam,
        post_choose_adam,
        create_first_generation,
        get_rate_songs,
        post_rate_songs,
        creating_next_generation_page,
        new_generation,
        ws
    ]
}