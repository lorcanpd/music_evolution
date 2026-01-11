# Slurm Job Offload Documentation

This document explains how the Music Evolution reproduction jobs are offloaded to the Slurm cluster in production mode.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                           martin (head node)                         │
│                                                                       │
│  ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐ │
│  │                 │     │                 │     │                 │ │
│  │  Docker         │     │  systemd timer  │────▶│  slurmctld      │ │
│  │  (web app)      │     │  (job runner)   │     │  (scheduler)    │ │
│  │                 │     │  runs as        │     │                 │ │
│  │  reads status   │◀────│  musicevo user  │     │                 │ │
│  └─────────────────┘     └─────────────────┘     └────────┬────────┘ │
│                                                           │          │
└───────────────────────────────────────────────────────────┼──────────┘
                                                            │
                          NFS (/srv/shared)                 │
                                                            │
┌───────────────────────────────────────────────────────────┼──────────┐
│                         worker01                          │          │
│                                                           ▼          │
│  ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐ │
│  │                 │     │                 │     │                 │ │
│  │  slurmd         │◀────│  Apptainer      │◀────│  reproduce      │ │
│  │  (executor)     │     │  (container)    │     │  (binary)       │ │
│  │  runs as        │     │                 │     │                 │ │
│  │  musicevo user  │     │                 │     │                 │ │
│  └─────────────────┘     └─────────────────┘     └─────────────────┘ │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘
```

**Key Design Principle**: The web container does NOT submit Slurm jobs directly. A host-side systemd timer runs the job runner script, which submits jobs to Slurm.

## Service User: musicevo

All Slurm jobs run as the dedicated `musicevo` user (UID/GID 2001).

### Creating the musicevo User

Run on ALL nodes (martin + all workers):

```bash
# Create group with fixed GID
sudo groupadd --gid 2001 musicevo

# Create user with fixed UID
sudo useradd --uid 2001 --gid 2001 \
    --home-dir /srv/services/music-evo \
    --shell /bin/bash \
    --system \
    musicevo

# Verify
id musicevo
# Should output: uid=2001(musicevo) gid=2001(musicevo) groups=2001(musicevo)
```

### Permissions Setup

```bash
# On martin - service directories
sudo chown -R musicevo:musicevo /srv/services/music-evo

# On all nodes - shared directories
sudo mkdir -p /srv/shared/jobs/music-evo
sudo mkdir -p /srv/shared/music-evo/current_generation
sudo chown -R musicevo:musicevo /srv/shared/jobs/music-evo
sudo chown -R musicevo:musicevo /srv/shared/music-evo
```

## How It Works

### 1. Job Runner (systemd timer)

The job runner runs on martin as a systemd timer:

1. **Timer** (`music-evo-jobrunner.timer`) fires every minute
2. **Service** (`music-evo-jobrunner.service`) runs as `musicevo` user
3. **Script** (`music-evo-jobrunner`) checks if reproduction is due:
   - Enforces `REPRO_INTERVAL_SECONDS` (default: 5 minutes)
   - Acquires lock (`/srv/shared/jobs/music-evo/repro.lock`)
   - Checks if songs exist to reproduce
   - Submits sbatch job to `musicevo` partition

### 2. Job Execution

On the worker node:

1. **Slurm** allocates resources on `musicevo` partition
2. **sbatch script** runs `run-reproduction.sbatch`
3. **Apptainer** (or native binary) executes the `reproduce` binary
4. **reproduce** binary:
   - Reads from PostgreSQL
   - Generates new songs (crossover + mutation)
   - Writes WAVs to `/srv/shared/music-evo/current_generation`
   - Updates status file

### 3. Status Reporting

The status file is at `/srv/shared/jobs/music-evo/current_status.json`:

```json
{
    "status": "completed",
    "message": "Generation 6 created successfully",
    "job_id": 12345,
    "current_generation": 5,
    "next_generation": 6,
    "timestamp": "2024-01-15T10:30:00Z",
    "node": "worker01"
}
```

The web app polls `/reproduction_status` endpoint which reads this file.

## Files and Paths

### On martin (host)

| Path | Purpose |
|------|---------|
| `/srv/services/music-evo/scripts/music-evo-jobrunner` | Job runner script |
| `/srv/services/music-evo/scripts/run-reproduction.sbatch` | sbatch script |
| `/etc/music-evo/jobrunner.env` | Configuration |
| `/etc/systemd/system/music-evo-jobrunner.service` | systemd service |
| `/etc/systemd/system/music-evo-jobrunner.timer` | systemd timer |

### Shared Storage (NFS)

| Path | Purpose |
|------|---------|
| `/srv/shared/jobs/music-evo/` | Job status and lock files |
| `/srv/shared/jobs/music-evo/logs/` | Slurm job stdout/stderr |
| `/srv/shared/music-evo/current_generation/` | Generated WAV files |
| `/srv/shared/images/music-evo.sif` | Apptainer container image |

## Setting Up the Job Runner

### 1. Copy Scripts

```bash
# Copy scripts
sudo cp deploy/scripts/music-evo-jobrunner /srv/services/music-evo/scripts/
sudo cp deploy/scripts/run-reproduction.sbatch /srv/services/music-evo/scripts/
sudo chmod +x /srv/services/music-evo/scripts/*

# Set ownership
sudo chown musicevo:musicevo /srv/services/music-evo/scripts/*
```

### 2. Create Configuration

```bash
sudo mkdir -p /etc/music-evo
sudo cp deploy/config/jobrunner.env.example /etc/music-evo/jobrunner.env
sudo nano /etc/music-evo/jobrunner.env
```

Edit the configuration:
```bash
# How often to run reproduction (seconds)
REPRO_INTERVAL_SECONDS=300

# Database connection (must be reachable from martin AND workers)
DATABASE_URL=postgres://musicevo:YOUR_PASSWORD@martin:5432/musicevo

# Slurm partition
SLURM_PARTITION=musicevo
```

### 3. Install systemd Units

```bash
sudo cp deploy/systemd/music-evo-jobrunner.service /etc/systemd/system/
sudo cp deploy/systemd/music-evo-jobrunner.timer /etc/systemd/system/

# Reload and enable
sudo systemctl daemon-reload
sudo systemctl enable music-evo-jobrunner.timer
sudo systemctl start music-evo-jobrunner.timer

# Check status
sudo systemctl status music-evo-jobrunner.timer
```

## Setting Up the Worker

### Option 1: Using Apptainer (Recommended)

Build and deploy a SIF image:

```bash
# Build Docker image (on dev machine or martin)
docker build -t music-evo:latest .

# Convert to SIF
/srv/shared/apptainer/bin/apptainer build \
    /srv/shared/images/music-evo.sif \
    docker-daemon://music-evo:latest

# Set permissions
sudo chown musicevo:musicevo /srv/shared/images/music-evo.sif
```

### Option 2: Native Binary

Build a statically-linked binary:

```bash
# On compatible build machine
cargo build --release --bin reproduce

# Copy to shared storage
sudo cp target/release/reproduce /srv/shared/music-evo/bin/
sudo chown musicevo:musicevo /srv/shared/music-evo/bin/reproduce
sudo chmod +x /srv/shared/music-evo/bin/reproduce
```

## The reproduce Binary

The `reproduce` binary runs the reproduction logic non-interactively:

```bash
reproduce --current-gen 5 --next-gen 6 --wav-dir /srv/shared/music-evo/current_generation
```

### Arguments

| Argument | Description |
|----------|-------------|
| `--current-gen <N>` | Current generation number |
| `--next-gen <M>` | Next generation to create |
| `--wav-dir <PATH>` | Directory to write WAV files |

### Environment

| Variable | Description |
|----------|-------------|
| `DATABASE_URL` | PostgreSQL connection string (required) |

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (see stderr) |

## Monitoring

### Check Timer Status

```bash
sudo systemctl status music-evo-jobrunner.timer
sudo systemctl list-timers | grep music-evo
```

### Check Recent Runs

```bash
sudo journalctl -u music-evo-jobrunner.service -n 50
```

### Check Job Queue

```bash
squeue -u musicevo
```

### Check Job Output

```bash
cat /srv/shared/jobs/music-evo/logs/repro-<job_id>.out
cat /srv/shared/jobs/music-evo/logs/repro-<job_id>.err
```

### Check Current Status

```bash
cat /srv/shared/jobs/music-evo/current_status.json | jq
```

## Troubleshooting

### Timer Not Running

```bash
# Check timer is enabled
sudo systemctl is-enabled music-evo-jobrunner.timer

# Check for errors
sudo journalctl -u music-evo-jobrunner.timer -n 20
```

### Job Runner Failing

```bash
# Run manually to see errors
sudo -u musicevo /srv/services/music-evo/scripts/music-evo-jobrunner
```

### Lock File Stuck

```bash
# Check if a job is running
squeue -u musicevo -n music-evo-repro

# If not, remove lock
sudo rm /srv/shared/jobs/music-evo/repro.lock
```

### Database Connection Issues

```bash
# Test from martin (as musicevo)
sudo -u musicevo psql "$DATABASE_URL" -c "SELECT 1"

# Test from worker
ssh worker01 "psql 'postgres://musicevo:PASSWORD@martin:5432/musicevo' -c 'SELECT 1'"
```

### Permission Denied

```bash
# Check ownership
ls -la /srv/shared/jobs/music-evo/
ls -la /srv/shared/music-evo/current_generation/

# Fix if needed
sudo chown -R musicevo:musicevo /srv/shared/jobs/music-evo/
sudo chown -R musicevo:musicevo /srv/shared/music-evo/
```

## Tuning REPRO_INTERVAL_SECONDS

The reproduction interval controls how often new generations are created.

```bash
# Edit configuration
sudo nano /etc/music-evo/jobrunner.env

# Change interval (in seconds)
REPRO_INTERVAL_SECONDS=600  # 10 minutes

# Restart timer to apply
sudo systemctl restart music-evo-jobrunner.timer
```

Considerations:
- **Shorter interval**: More generations, but less ratings per generation
- **Longer interval**: More ratings per generation, slower evolution
- **Recommended**: Start with 300-600 seconds and adjust based on usage

## Development Mode (No Slurm)

For local development, the app runs in development mode by default:

- `PRODUCTION_MODE=false` (default)
- Reproduction is triggered when ratings reach threshold
- No Slurm, no job runner, no external dependencies

To test locally:
```bash
docker compose up --build
# Reproduction happens automatically when enough ratings are collected
```
