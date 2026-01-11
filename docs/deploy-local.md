# Local Development Deployment

This guide explains how to run Music Evolution locally using Docker Compose.

## Prerequisites

- Docker Engine 20.10+
- Docker Compose v2+
- Git

## Quick Start

1. **Clone the repository** (if you haven't already):
   ```bash
   git clone <repo-url>
   cd music_evo
   ```

2. **Create environment file**:
   ```bash
   cp .env.example .env
   # Edit .env if you want to change defaults (optional for local dev)
   ```

3. **Build and start the services**:
   ```bash
   docker compose up --build
   ```

   This will:
   - Build the Rust application in a multi-stage Docker build
   - Start PostgreSQL database
   - Start the Nginx reverse proxy
   - Start the web application

4. **Access the application**:
   Open your browser to: http://localhost:8080

## Development Mode (No Slurm)

In development mode (`PRODUCTION_MODE=false`, the default), the application handles reproduction internally:

- **Threshold-based reproduction**: When enough ratings are collected, reproduction is triggered automatically
- **No external job runner**: Everything runs inside Docker
- **Immediate feedback**: New generations are created on-demand

This differs from production mode, which uses a systemd timer and Slurm cluster for reproduction. See [deploy-martin.md](deploy-martin.md) for production setup.

## First-Time Setup

When you first access the application, you'll be guided through:

1. **Initialize the experiment** - Creates database tables
2. **Choose Adam** - Listen to randomly generated songs and pick the ancestor
3. **Create first generation** - Generate the initial population
4. **Rate songs** - Start rating songs to guide evolution

## Service Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│                 │     │                 │     │                 │
│  Browser        │────▶│  Nginx Proxy    │────▶│  Rocket App     │
│  localhost:8080 │     │  (rate limiting)│     │  (port 8000)    │
│                 │     │                 │     │                 │
└─────────────────┘     └─────────────────┘     └────────┬────────┘
                                                         │
                                                         ▼
                                                ┌─────────────────┐
                                                │                 │
                                                │  PostgreSQL     │
                                                │  (port 5432)    │
                                                │                 │
                                                └─────────────────┘
```

## Useful Commands

### View logs
```bash
# All services
docker compose logs -f

# Specific service
docker compose logs -f app
docker compose logs -f postgres
docker compose logs -f proxy
```

### Restart services
```bash
docker compose restart
```

### Rebuild after code changes
```bash
docker compose up --build
```

### Stop services
```bash
docker compose down
```

### Stop and remove volumes (reset database)
```bash
docker compose down -v
```

### Shell into the app container
```bash
docker compose exec app bash
```

## Development Tips

### Hot-reload static files

The `static/` directory is mounted as a volume, so CSS changes take effect immediately (just refresh the browser).

### Viewing the database

```bash
# Connect to PostgreSQL
docker compose exec postgres psql -U musicevo -d musicevo

# Common queries
SELECT COUNT(*) FROM songs;
SELECT generation, COUNT(*) FROM songs GROUP BY generation;
SELECT * FROM current_generation_fitness;
```

### Rebuilding just the app

```bash
docker compose build app
docker compose up -d app
```

## Troubleshooting

### Port already in use

If port 8080 is in use, change it in `.env`:
```bash
HTTP_PORT=3000
```

### Database connection errors

The app waits for PostgreSQL to be healthy before starting. If you see connection errors:

```bash
# Check postgres logs
docker compose logs postgres

# Restart everything
docker compose down && docker compose up
```

### Build failures

If the Rust build fails:

```bash
# Clean rebuild
docker compose build --no-cache app
```

### Audio files not playing

Check that the `current_generation/` directory exists and has WAV files:

```bash
ls -la current_generation/
```

If empty, the first generation hasn't been created yet. Complete the setup flow.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_USER` | `musicevo` | Database username |
| `POSTGRES_PASSWORD` | `devpassword` | Database password |
| `POSTGRES_DB` | `musicevo` | Database name |
| `HTTP_PORT` | `8080` | Port for the proxy |
| `PRODUCTION_MODE` | `false` | Enable production mode (external job runner) |
| `REPRODUCTION_STATUS_FILE` | `/app/reproduction_status.json` | Path to status file (production only) |
