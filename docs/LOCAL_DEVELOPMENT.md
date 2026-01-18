# Local Development Guide

Quick reference for running and testing Music Evolution locally using Docker Compose.

## Quick Start

```bash
# Start everything (first time or after code changes)
docker compose up --build -d

# View logs
docker compose logs -f app

# Access the app
open http://localhost:8080
```

## Common Operations

### Full Reset (Database + Audio + Data)

When you need a completely fresh start:

```bash
# Stop and remove everything including volumes
docker compose down -v

# Remove local audio and data directories
rm -rf audio/generations audio/current data/greatest_hits/revisions data/greatest_hits/current

# Recreate directory structure
mkdir -p audio/generations data/greatest_hits/revisions

# Start fresh
docker compose up --build -d
```

### Database Only Reset

Keep your code but reset the experiment:

```bash
# Stop containers
docker compose down

# Remove only the postgres volume
docker volume rm music_evo_postgres_data 2>/dev/null || true

# Clean audio files
rm -rf audio/generations/* audio/current data/greatest_hits/revisions/* data/greatest_hits/current

# Recreate directories
mkdir -p audio/generations data/greatest_hits/revisions

# Restart
docker compose up -d
```

### Rebuild After Code Changes

```bash
# Rebuild and restart app only (faster)
docker compose up --build -d app

# Or rebuild everything
docker compose up --build -d
```

### View Logs

```bash
# All services
docker compose logs -f

# App only
docker compose logs -f app

# Database only
docker compose logs -f postgres

# Last 100 lines
docker compose logs --tail=100 app
```

### Access Database Directly

```bash
# Connect to postgres
docker compose exec postgres psql -U musicevo -d musicevo

# Common queries:
# - Check tables: \dt
# - Count songs: SELECT COUNT(*), generation FROM songs GROUP BY generation;
# - Check ratings: SELECT * FROM current_generation_fitness;
# - Check historic: SELECT * FROM historic_fitness_scores;
```

### Quick Database Inspection Script

```bash
# Create a helper script
cat > db-status.sh << 'EOF'
#!/bin/bash
docker compose exec -T postgres psql -U musicevo -d musicevo << SQL
SELECT 'Tables' as info;
\dt
SELECT 'Songs by generation' as info;
SELECT generation, COUNT(*) as songs FROM songs GROUP BY generation ORDER BY generation;
SELECT 'Current ratings' as info;
SELECT COUNT(*) as total_ratings FROM current_generation_fitness;
SELECT 'Historic scores' as info;
SELECT COUNT(*) as archived_songs FROM historic_fitness_scores;
SQL
EOF
chmod +x db-status.sh
```

## Troubleshooting

### "relation does not exist" Error

The database tables haven't been created. Visit the root URL `/` and you'll be redirected to `/initialise_experiment` to set up the database.

If that doesn't work:
```bash
# Reset database
docker compose down
docker volume rm music_evo_postgres_data
docker compose up -d
```

### "Device or resource busy" Error

Old audio cleanup issue (should be fixed). If it occurs:
```bash
# Restart the app
docker compose restart app
```

### App Not Responding / 502 Error

```bash
# Check if app is running
docker compose ps

# Check app logs for errors
docker compose logs --tail=50 app

# Restart app
docker compose restart app
```

### Database Connection Failed

```bash
# Check postgres is healthy
docker compose ps postgres

# Check postgres logs
docker compose logs postgres

# Restart postgres (will lose data unless you have volume)
docker compose restart postgres
```

### Port Already in Use

```bash
# Change the port in docker-compose.yml or use env var
HTTP_PORT=9090 docker compose up -d
```

## Development Workflow

### Typical Testing Cycle

1. Make code changes
2. Rebuild: `docker compose up --build -d app`
3. Check logs: `docker compose logs -f app`
4. Test in browser: `http://localhost:8080`
5. If needed, reset DB and repeat

### Testing Reproduction

The reproduction threshold is `2 * number_of_songs` ratings. For generation 1 with 44 songs, you need 88 ratings.

To speed up testing:
```bash
# Connect to DB and manually insert ratings
docker compose exec postgres psql -U musicevo -d musicevo -c "
INSERT INTO current_generation_fitness (song_id, rating)
SELECT song_id, (random() > 0.5)::int
FROM songs
WHERE generation = (SELECT MAX(generation) FROM songs)
CROSS JOIN generate_series(1, 2);
"
```

### Testing Greatest Hits

Greatest hits updates after each reproduction. To manually trigger (requires at least one generation with ratings):

```bash
# Check current greatest hits data
ls -la data/greatest_hits/
cat data/greatest_hits/current/metadata.json 2>/dev/null || echo "No greatest hits yet"
```

## File Locations

| Purpose | Local Path | Container Path |
|---------|-----------|----------------|
| Audio files | `./audio/` | `/app/audio/` |
| Greatest hits | `./data/` | `/app/data/` |
| Static CSS/JS | `./static/` | `/app/static/` |
| Habitat config | `./habitat_config.json` | `/app/habitat_config.json` |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HTTP_PORT` | `8080` | Host port for web access |
| `POSTGRES_USER` | `musicevo` | Database user |
| `POSTGRES_PASSWORD` | `devpassword` | Database password |
| `POSTGRES_DB` | `musicevo` | Database name |
| `PRODUCTION_MODE` | `false` | Enable production mode |

## Container Commands

```bash
# Shell into app container
docker compose exec app bash

# Shell into postgres container
docker compose exec postgres bash

# Run a one-off command
docker compose run --rm app ./scrub_db
```
