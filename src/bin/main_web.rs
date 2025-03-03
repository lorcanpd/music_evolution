// src/bin/main_web.rs

#[macro_use] extern crate rocket;
#[macro_use] extern crate lazy_static;
extern crate rand;
extern crate maud;
extern crate tokio_postgres;

use dotenv::dotenv;
use rocket::fairing::AdHoc;
use rocket::tokio::sync::broadcast;
use deadpool_postgres::{Config as DpPgConfig, Pool, Runtime};
use tokio_postgres::{NoTls, Config as PgClientConfig};
use music_evo::web_interface;
use music_evo::user_interaction::AppState;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;

#[launch]
async fn rocket() -> _ {
    dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    let client_config: PgClientConfig = database_url.parse().expect("Invalid DB URL");
    let mut pg_cfg = DpPgConfig::new();
    pg_cfg.host = client_config.get_hosts().get(0).and_then(|host| {
        if let tokio_postgres::config::Host::Tcp(host_str) = host {
            Some(host_str.to_string())
        } else {
            None
        }
    });
    pg_cfg.port = client_config.get_ports().get(0).cloned();
    pg_cfg.user = client_config.get_user().map(|s| s.to_string());
    pg_cfg.password = client_config.get_password().map(|s| String::from_utf8_lossy(s).to_string());
    pg_cfg.dbname = client_config.get_dbname().map(|s| s.to_string());

    let pool: Pool = pg_cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .expect("Error creating pool");

    let (notify_tx, _) = broadcast::channel::<()>(10);

    rocket::build()
        .manage(AppState {
            pool,
            audio_cache: RwLock::new(HashMap::new()),
        })
        .manage(notify_tx)
        .mount("/", web_interface::routes())
}


