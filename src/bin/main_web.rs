// src/bin/main_web.rs
#[macro_use] extern crate rocket;  // for routes and macros
#[macro_use] extern crate lazy_static; // for the global static
extern crate rand;
extern crate maud;
extern crate tokio_postgres;


use dotenv::dotenv;
use rocket::tokio::sync::broadcast;
use rocket::fairing::AdHoc;
use tokio_postgres::{NoTls, Client};

use music_evo::web_interface;
use music_evo::user_interaction::AppState;

#[launch]
async fn rocket() -> _ {
    // read DB URL from env
    dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    // Connect to the database asynchronously
    let (client, connection) = tokio_postgres::connect(&database_url, NoTls).await.unwrap();

    // Spawn the connection to run in the background
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    // Create a broadcast channel for WebSocket notifications
    let (notify_tx, _) = broadcast::channel::<()>(10);

    rocket::build()
        // store DB URL and WebSocket sender in a managed state
        .manage(AppState { client })
        .manage(notify_tx)
        // mount all routes from `web_interface`
        .mount("/", web_interface::routes())
}