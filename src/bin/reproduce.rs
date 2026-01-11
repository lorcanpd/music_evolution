// src/bin/reproduce.rs
//
// Non-interactive reproduction binary for Slurm job execution.
// Usage: reproduce --current-gen <N> --next-gen <M> --wav-dir <PATH>
//
// Reads DATABASE_URL from environment.
// Exits 0 on success, non-zero on failure with stderr output.

use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

use deadpool_postgres::{Config as DpPgConfig, Pool, Runtime};
use tokio_postgres::{Config as PgClientConfig, NoTls};

use music_evo::reproduction::run_reproduction;

#[derive(Debug)]
struct Args {
    current_gen: i32,
    next_gen: i32,
    wav_dir: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = env::args().collect();

    let mut current_gen: Option<i32> = None;
    let mut next_gen: Option<i32> = None;
    let mut wav_dir: Option<PathBuf> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--current-gen" => {
                i += 1;
                if i >= args.len() {
                    return Err("--current-gen requires a value".to_string());
                }
                current_gen = Some(args[i].parse().map_err(|_| "Invalid --current-gen value")?);
            }
            "--next-gen" => {
                i += 1;
                if i >= args.len() {
                    return Err("--next-gen requires a value".to_string());
                }
                next_gen = Some(args[i].parse().map_err(|_| "Invalid --next-gen value")?);
            }
            "--wav-dir" => {
                i += 1;
                if i >= args.len() {
                    return Err("--wav-dir requires a value".to_string());
                }
                wav_dir = Some(PathBuf::from(&args[i]));
            }
            "--help" | "-h" => {
                eprintln!("Usage: reproduce --current-gen <N> --next-gen <M> --wav-dir <PATH>");
                eprintln!();
                eprintln!("Options:");
                eprintln!("  --current-gen <N>   Current generation number");
                eprintln!("  --next-gen <M>      Next generation number to create");
                eprintln!("  --wav-dir <PATH>    Directory to write WAV files");
                eprintln!();
                eprintln!("Environment:");
                eprintln!("  DATABASE_URL        PostgreSQL connection string (required)");
                std::process::exit(0);
            }
            arg => {
                return Err(format!("Unknown argument: {}", arg));
            }
        }
        i += 1;
    }

    Ok(Args {
        current_gen: current_gen.ok_or("--current-gen is required")?,
        next_gen: next_gen.ok_or("--next-gen is required")?,
        wav_dir: wav_dir.ok_or("--wav-dir is required")?,
    })
}

async fn create_pool() -> Result<Pool, Box<dyn Error>> {
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL environment variable not set")?;

    let client_config: PgClientConfig = database_url.parse()
        .map_err(|e| format!("Invalid DATABASE_URL: {}", e))?;

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

    let pool = pg_cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .map_err(|e| format!("Failed to create connection pool: {}", e))?;

    Ok(pool)
}

#[tokio::main]
async fn main() -> ExitCode {
    // Parse arguments
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!("Use --help for usage information");
            return ExitCode::from(1);
        }
    };

    eprintln!("Starting reproduction: generation {} -> {}", args.current_gen, args.next_gen);
    eprintln!("WAV output directory: {}", args.wav_dir.display());

    // Validate wav_dir exists or create it
    if let Err(e) = std::fs::create_dir_all(&args.wav_dir) {
        eprintln!("Error: Failed to create WAV directory: {}", e);
        return ExitCode::from(1);
    }

    // Create database pool
    let pool = match create_pool().await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::from(1);
        }
    };

    // Test database connection
    match pool.get().await {
        Ok(_) => eprintln!("Database connection successful"),
        Err(e) => {
            eprintln!("Error: Failed to connect to database: {}", e);
            return ExitCode::from(1);
        }
    }

    // Run reproduction
    match run_reproduction(args.current_gen, args.next_gen, &pool, &args.wav_dir).await {
        Ok(song_count) => {
            eprintln!("Reproduction completed successfully. Created {} songs.", song_count);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: Reproduction failed: {}", e);
            ExitCode::from(1)
        }
    }
}
