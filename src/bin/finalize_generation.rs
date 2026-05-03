use std::env;
use std::error::Error;
use std::process::ExitCode;

use deadpool_postgres::{Config as DpPgConfig, Pool, Runtime};
use tokio_postgres::{Config as PgClientConfig, NoTls};

use music_evo::database::create_database;
use music_evo::family_trees;

#[derive(Debug)]
struct Args {
    previous_gen: i32,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = env::args().collect();
    let mut previous_gen: Option<i32> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--previous-gen" => {
                i += 1;
                if i >= args.len() {
                    return Err("--previous-gen requires a value".to_string());
                }
                previous_gen = Some(args[i].parse().map_err(|_| "Invalid --previous-gen value")?);
            }
            "--help" | "-h" => {
                eprintln!("Usage: finalize_generation --previous-gen <N>");
                eprintln!();
                eprintln!("Environment:");
                eprintln!("  DATABASE_URL        PostgreSQL connection string (required)");
                std::process::exit(0);
            }
            arg => return Err(format!("Unknown argument: {}", arg)),
        }
        i += 1;
    }

    Ok(Args {
        previous_gen: previous_gen.ok_or("--previous-gen is required")?,
    })
}

async fn create_pool() -> Result<Pool, Box<dyn Error>> {
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL environment variable not set")?;

    let client_config: PgClientConfig = database_url.parse()
        .map_err(|e| format!("Invalid DATABASE_URL: {}", e))?;

    let mut pg_cfg = DpPgConfig::new();
    pg_cfg.host = client_config.get_hosts().first().and_then(|host| {
        if let tokio_postgres::config::Host::Tcp(host_str) = host {
            Some(host_str.to_string())
        } else {
            None
        }
    });
    pg_cfg.port = client_config.get_ports().first().copied();
    pg_cfg.user = client_config.get_user().map(|s| s.to_string());
    pg_cfg.password = client_config.get_password().map(|s| String::from_utf8_lossy(s).to_string());
    pg_cfg.dbname = client_config.get_dbname().map(|s| s.to_string());

    Ok(pg_cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .map_err(|e| format!("Failed to create connection pool: {}", e))?)
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("Error: {}", error);
            eprintln!("Use --help for usage information");
            return ExitCode::from(1);
        }
    };

    let pool = match create_pool().await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("Error: {}", error);
            return ExitCode::from(1);
        }
    };

    if let Err(error) = create_database(&pool).await {
        eprintln!("Error: failed to ensure database schema: {}", error);
        return ExitCode::from(1);
    }

    match family_trees::update_family_trees_safe(&pool, args.previous_gen).await {
        Ok(()) => {
            eprintln!(
                "Family tree package finalized successfully for generation {}",
                args.previous_gen
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Error: family tree finalization failed: {}", error);
            ExitCode::from(1)
        }
    }
}
