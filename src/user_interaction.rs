// src/user_interaction.rs

use std::io::Write;
use std::error::Error;
use lazy_static::lazy_static;
use rand::Rng;
use rocket::{get, post, State};
use rocket::form::{Form, FromForm};
use rocket::response::{Redirect, content::RawHtml, status};
use maud::{html, Markup};

// FromForm

// use postgres::{Client as SyncClient, NoTls};
use tokio_postgres::{Client as AsyncClient, NoTls, connect};


use std::io;
use std::sync::Mutex;
use crate::genome::Genome;
use crate::decode_genome::DecodedGenome;
use crate::play_genes::{generate_wav, play_genes, play_precomputed_wav};



// ------------------------------------------
// GLOBAL STATIC to store the "Adam" we last generated
// ------------------------------------------
lazy_static! {
    static ref CURRENT_ADAM: Mutex<Option<Genome>> = Mutex::new(None);
}

// We'll also store a small "AppState" for DB connection, if needed:

pub struct AppState {
    pub client: AsyncClient,
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
//  - Generate a random Adam
//  - Store it in CURRENT_ADAM
//  - Generate a "temp_adam.wav" so user can listen
//  - Return an HTML page with an <audio> element + yes/no form
// ------------------------------------------
#[get("/choose_adam")]
pub fn get_choose_adam(state: &State<AppState>) -> Result<RawHtml<String>, status::Custom<String>> {
    // 1. Generate random Adam
    let mut adam = Genome::initialise_random_genome(
        128, 256, 8, 16);

    // For demonstration, set a random mutation rate
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
    //    We'll serve the file via e.g. GET /temp_adam.wav using rocket's static files or NamedFile
    //    For brevity, let's assume we do that in main or a route.
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
//  - Receives user response "yes" or "no"
//  - If "yes", insert Adam into DB as generation=0
//  - If "no", redirect back to GET /choose_adam for a new random Adam
// ------------------------------------------
#[post("/choose_adam", data="<form_data>")]
pub async fn post_choose_adam(
    state: &State<AppState>, form_data: Form<AdamResponseForm>
)-> Result<Redirect, status::Custom<String>>
{
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
            return Err(status::Custom(
                rocket::http::Status::InternalServerError,
                "No Adam in memory".to_string()
            ));
        }
    };

    // set song_id to 1 for Adam
    adam.assign_song_id(1);

    let client = &state.client;

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

    // set song_id to 2 for Eve
    eve.assign_song_id(2);

    // store eve in the database
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
//  - Pick a random song from DB
//  - Return an HTML page with an <audio> element + yes/no form
// ------------------------------------------
#[get("/rate_songs")]
pub async fn get_rate_songs(state: &State<AppState>) -> Result<RawHtml<String>, status::Custom<String>> {
    let client = &state.client;

    let song_id: i32 = {
        let row = client.query_one(
            "SELECT song_id FROM songs
            WHERE generation = (SELECT MAX(generation) FROM songs)
            ORDER BY RANDOM()
            LIMIT 1",
            &[]
        ).await.map_err(|e| {
            status::Custom(
                rocket::http::Status::InternalServerError,
                format!("Failed to get random song: {}", e)
            )
        })?;
        row.get("song_id")
    };

    let markup: Markup = html! {
        html {
            head {
                title { "Rate a Song" }
            }
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
//  - Receives user response "yes" or "no"
//  - Insert rating into DB
//  - Redirect back to GET /rate_songs for a new random song
// ------------------------------------------

#[post("/rate_songs", data = "<form_data>")]
pub async fn post_rate_songs(
    state: &State<AppState>, form_data: Form<RatingForm>
) -> Result<Redirect, status::Custom<String>> {
    let song_id = form_data.song_id;
    let rating = form_data.rating;

    // Get the asynchronous client from the state
    let client = &state.client;

    // Insert rating into DB asynchronously
    match client.execute(
        "INSERT INTO current_generation_fitness (song_id, rating)
        VALUES ($1, $2)",
        &[&song_id, &rating],
    ).await {
        Ok(_) => {
            println!("Rating inserted for song_id={}", song_id);
        }
        Err(e) => {
            return Err(status::Custom(
                rocket::http::Status::InternalServerError,
                format!("Failed to insert rating: {}", e)
            ));
        }
    }

    // check to see if we have reached 3 times the number of songs in the habitat in ratings.
    let total_songs: i64 = 4;  // TODO this is a placeholder for now.
    let total_ratings: i64 = client.query_one(
        "SELECT COUNT(*) as count FROM current_generation_fitness",
        &[]
    ).await.map_err(|e| {
        status::Custom(
            rocket::http::Status::InternalServerError,
            format!("Failed to get total ratings: {}", e)
        )
    })?.get("count");

    if total_ratings >= total_songs {
        return Ok(Redirect::to("/creating_next_generation"));
    }

    // Redirect back to GET /rate_songs
    Ok(Redirect::to("/rate_songs"))
}

// ------------------------------------------
// Comands for user input throught the terminal
// ------------------------------------------

pub fn choose_adam() -> Result<Genome, Box<dyn Error>> {
    loop {
        // Generate a random Adam genome
        let mut adam = Genome::initialise_random_genome(
            128, 256, 8, 16);
        adam.assign_mutation_rate(0.02);

        println!("Accept this Adam? (N/y): ");
        // play adam
        let decoded = DecodedGenome::decode(&adam);
        play_genes(&decoded)?;

        // Prompt user
        print!("> ");
        io::stdout().flush()?; // ensure prompt is displayed

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input.is_empty() || input == "n" || input == "no" {
            println!("Generating a new Adam...");
            // loop continues, generating a new one
        } else if input == "y" || input == "yes" {
            println!("Adam accepted.");
            return Ok(adam);
        } else {
            println!("Please type 'y' to approve or 'n' to reject.");
        }
    }
}

/// Randomly sample and rate songs until a given rating limit is reached.
/// For each rating, store it in the `current_generation_fitness` table.
///
/// - `rating_limit`: number of total ratings to collect before stopping.
/// - After collecting `rating_limit` ratings, scrub the database (for now).
pub async fn rate_songs(rating_limit: i32) -> Result<(), Box<dyn Error>> {
    // Retrieve the DATABASE_URL environment variable
    let database_url = std::env::var("DATABASE_URL")?;
    let (client, connection) = connect(&database_url, NoTls).await?;

    // Spawn the connection to run in the background
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    let mut rng = rand::thread_rng();
    let mut ratings_collected = 0;

    println!("Starting rating process. Press 'q' to quit early.");

    while ratings_collected < rating_limit {
        // Get the song_ids of the current generation of songs
        let song_ids_rows = client.query("SELECT song_id FROM songs WHERE generation = (SELECT MAX(generation) FROM songs)", &[]).await?;

        // Get song_ids from the rows
        let song_ids: Vec<i32> = song_ids_rows.iter().map(|row| row.get("song_id")).collect();

        // If for some reason no row returned, continue
        if song_ids.is_empty() {
            println!("No songs in the database. Exiting...");
            break;
        }

        // Pick a random song_id
        let song_id = song_ids[rng.gen_range(0..song_ids.len())];

        // Play the song
        println!("Playing song_id={}", song_id);
        play_precomputed_wav(song_id)?;

        // Prompt the user for a rating
        println!("Do you like song {}? N/y or q to quit:", song_id);

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("q") {
            println!("Quitting rating early...");
            break;
        }

        // Parse as i32
        let rating: i32 = loop {
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
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

        // Store rating in current_generation_fitness
        client.execute(
            "INSERT INTO current_generation_fitness (song_id, rating)
            VALUES ($1, $2)",
            &[&song_id, &rating],
        ).await?;

        ratings_collected += 1;
        println!("Recorded rating for song {}, total ratings = {}", song_id, ratings_collected);
    }

    Ok(())
}