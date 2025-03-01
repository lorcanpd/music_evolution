// src/bin/scrub_db.rs

use music_evo::initialise_experiment::scrub_database;
use deadpool_postgres::{Config as DpPgConfig, Pool, Runtime};
use tokio_postgres::{NoTls, Config as PgClientConfig};
use dotenv::dotenv;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

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

    if let Err(e) = scrub_database(&pool).await {
        eprintln!("Error: {}", e);
    }
}


