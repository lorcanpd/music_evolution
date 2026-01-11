# Deploying to Martin (Raspberry Pi 5)

This guide explains how to deploy Music Evolution to your Raspberry Pi 5 head node "martin".

## Prerequisites

On martin:
- Docker Engine 20.10+ (arm64)
- Docker Compose v2+
- Slurm (slurmctld running)
- NVMe SSD mounted at `/srv/services`
- NFS share configured at `/srv/shared`

On worker nodes:
- Slurm (slurmd running)
- Access to NFS share at `/srv/shared`
- Apptainer (optional, for containerized reproduction)

## Step 0: Create the musicevo Service User

**IMPORTANT**: This user must exist on ALL nodes (martin + all workers) with the same UID/GID.

Run on **every node** in the cluster:

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

## Directory Structure on Martin

Create the following directory structure on the NVMe:

```bash
sudo mkdir -p /srv/services/music-evo/{postgres,nginx/conf.d,certs,config,scripts,acme-challenge}
sudo chown -R musicevo:musicevo /srv/services/music-evo
```

Create shared directories (on NFS):

```bash
sudo mkdir -p /srv/shared/jobs/music-evo/logs
sudo mkdir -p /srv/shared/music-evo/current_generation
sudo mkdir -p /srv/shared/images
sudo chown -R musicevo:musicevo /srv/shared/jobs/music-evo
sudo chown -R musicevo:musicevo /srv/shared/music-evo
```

## Step 1: Copy Files to Martin

From your development machine:

```bash
# Create a deployment bundle
cd /path/to/music_evo
tar -czvf music-evo-deploy.tar.gz \
    Dockerfile \
    Cargo.toml \
    Cargo.lock \
    src/ \
    static/ \
    habitat_config.json \
    docker-compose.yml \
    compose.prod.yml \
    deploy/ \
    .env.example

# Copy to martin
scp music-evo-deploy.tar.gz martin:/srv/services/music-evo/
```

On martin:
```bash
cd /srv/services/music-evo
tar -xzvf music-evo-deploy.tar.gz
```

## Step 2: Configure Environment

```bash
cd /srv/services/music-evo
cp .env.example .env
nano .env
```

**Edit the `.env` file with production values:**

```bash
# IMPORTANT: Use strong passwords!
POSTGRES_USER=musicevo
POSTGRES_PASSWORD=YOUR_STRONG_PASSWORD_HERE
POSTGRES_DB=musicevo

# Production mode - reproduction handled by external job runner
PRODUCTION_MODE=true

# Status file (on NFS, written by job runner, read by app)
REPRODUCTION_STATUS_FILE=/srv/shared/jobs/music-evo/current_status.json
```

## Step 3: Set Up Nginx and Job Runner

```bash
# Copy nginx configs
cp -r deploy/nginx/* /srv/services/music-evo/nginx/

# Copy habitat config
cp habitat_config.json /srv/services/music-evo/config/

# Copy job runner scripts
sudo cp deploy/scripts/music-evo-jobrunner /srv/services/music-evo/scripts/
sudo cp deploy/scripts/run-reproduction.sbatch /srv/services/music-evo/scripts/
sudo chmod +x /srv/services/music-evo/scripts/*
sudo chown musicevo:musicevo /srv/services/music-evo/scripts/*
```

For HTTP-only (no TLS):
```bash
# The default config works for HTTP
```

For HTTPS (after obtaining certificates, see public-access.md):
```bash
# Copy the TLS config template
cp deploy/nginx/conf.d/default-tls.conf.example /srv/services/music-evo/nginx/conf.d/default.conf

# Edit and replace YOUR_DOMAIN.example.com with your actual domain
nano /srv/services/music-evo/nginx/conf.d/default.conf
```

## Step 4: Build and Start

```bash
cd /srv/services/music-evo

# Build the application (this will take a while on Raspberry Pi)
docker compose -f docker-compose.yml -f compose.prod.yml build

# Start services
docker compose -f docker-compose.yml -f compose.prod.yml up -d
```

## Step 5: Verify Deployment

```bash
# Check service status
docker compose -f docker-compose.yml -f compose.prod.yml ps

# View logs
docker compose -f docker-compose.yml -f compose.prod.yml logs -f

# Test the application
curl http://localhost/
```

## Step 6: Set Up Slurm Job Runner

The reproduction jobs are submitted by a systemd timer running on the host (not from inside Docker).

### Configure the Job Runner

```bash
# Create configuration directory
sudo mkdir -p /etc/music-evo

# Copy configuration template
sudo cp deploy/config/jobrunner.env.example /etc/music-evo/jobrunner.env

# Edit with your settings
sudo nano /etc/music-evo/jobrunner.env
```

**Edit `/etc/music-evo/jobrunner.env`:**

```bash
# How often to run reproduction (seconds) - default 5 minutes
REPRO_INTERVAL_SECONDS=300

# Database connection (must be reachable from martin AND workers)
DATABASE_URL=postgres://musicevo:YOUR_PASSWORD@localhost:5432/musicevo

# Slurm partition for reproduction jobs
SLURM_PARTITION=musicevo
```

### Install systemd Timer

```bash
# Copy systemd units
sudo cp deploy/systemd/music-evo-jobrunner.service /etc/systemd/system/
sudo cp deploy/systemd/music-evo-jobrunner.timer /etc/systemd/system/

# Reload systemd
sudo systemctl daemon-reload

# Enable and start the timer
sudo systemctl enable music-evo-jobrunner.timer
sudo systemctl start music-evo-jobrunner.timer

# Verify it's running
sudo systemctl status music-evo-jobrunner.timer
sudo systemctl list-timers | grep music-evo
```

### Verify Job Runner

```bash
# Run manually to test
sudo -u musicevo /srv/services/music-evo/scripts/music-evo-jobrunner

# Check job queue
squeue -u musicevo

# Check status file
cat /srv/shared/jobs/music-evo/current_status.json | jq
```

### Tuning REPRO_INTERVAL_SECONDS

The interval controls how often new generations are created:

- **Shorter interval** (e.g., 120s): More generations, fewer ratings per generation
- **Longer interval** (e.g., 600s): More ratings per generation, slower evolution
- **Recommended**: Start with 300-600 seconds and adjust based on traffic

To change the interval:

```bash
sudo nano /etc/music-evo/jobrunner.env
# Change REPRO_INTERVAL_SECONDS value
sudo systemctl restart music-evo-jobrunner.timer
```

For detailed Slurm architecture and troubleshooting, see [slurm-offload.md](slurm-offload.md).

## Managing the Deployment

### Start/Stop/Restart

```bash
cd /srv/services/music-evo

# Start
docker compose -f docker-compose.yml -f compose.prod.yml up -d

# Stop
docker compose -f docker-compose.yml -f compose.prod.yml down

# Restart
docker compose -f docker-compose.yml -f compose.prod.yml restart
```

### Update Application

```bash
# Pull new code (or scp new tarball)
cd /srv/services/music-evo

# Rebuild and restart
docker compose -f docker-compose.yml -f compose.prod.yml build app
docker compose -f docker-compose.yml -f compose.prod.yml up -d app
```

### View Logs

```bash
# All services
docker compose -f docker-compose.yml -f compose.prod.yml logs -f

# Just the app
docker compose -f docker-compose.yml -f compose.prod.yml logs -f app

# Check nginx access logs
docker compose -f docker-compose.yml -f compose.prod.yml exec proxy cat /var/log/nginx/access.log
```

### Database Backup

```bash
# Create backup
docker compose -f docker-compose.yml -f compose.prod.yml exec postgres \
    pg_dump -U musicevo musicevo > backup_$(date +%Y%m%d).sql

# Restore backup
cat backup_20240101.sql | docker compose -f docker-compose.yml -f compose.prod.yml exec -T postgres \
    psql -U musicevo musicevo
```

## Auto-Start on Boot

There are two services to enable:

### 1. Docker Compose Service (web app + database)

Create a systemd service:

```bash
sudo nano /etc/systemd/system/music-evo.service
```

```ini
[Unit]
Description=Music Evolution Web Application
Requires=docker.service
After=docker.service

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=/srv/services/music-evo
ExecStart=/usr/bin/docker compose -f docker-compose.yml -f compose.prod.yml up -d
ExecStop=/usr/bin/docker compose -f docker-compose.yml -f compose.prod.yml down

[Install]
WantedBy=multi-user.target
```

Enable it:
```bash
sudo systemctl daemon-reload
sudo systemctl enable music-evo
```

### 2. Job Runner Timer (already set up in Step 6)

The job runner timer should already be enabled from Step 6:
```bash
# Verify it's enabled
sudo systemctl is-enabled music-evo-jobrunner.timer
```

Both services will start automatically on boot.

## Troubleshooting

### Container won't start

```bash
# Check logs
docker compose -f docker-compose.yml -f compose.prod.yml logs app

# Check if postgres is ready
docker compose -f docker-compose.yml -f compose.prod.yml exec postgres pg_isready
```

### Database issues

```bash
# Connect to database
docker compose -f docker-compose.yml -f compose.prod.yml exec postgres \
    psql -U musicevo -d musicevo

# Check tables
\dt
SELECT COUNT(*) FROM songs;
```

### Nginx errors

```bash
# Check config syntax
docker compose -f docker-compose.yml -f compose.prod.yml exec proxy nginx -t

# View error log
docker compose -f docker-compose.yml -f compose.prod.yml exec proxy cat /var/log/nginx/error.log
```

### Slurm job issues

```bash
# Check job queue
squeue -u musicevo

# Check job history
sacct -u musicevo

# View job output
cat /srv/shared/jobs/music-evo/logs/repro-<job_id>.out
cat /srv/shared/jobs/music-evo/logs/repro-<job_id>.err

# Check job runner logs
sudo journalctl -u music-evo-jobrunner.service -n 50

# Check timer status
sudo systemctl status music-evo-jobrunner.timer
```

### Lock file stuck

If reproduction seems stuck, check for a stale lock:

```bash
# Check if a job is currently running
squeue -u musicevo -n music-evo-repro

# If no job is running but lock exists, remove it
ls -la /srv/shared/jobs/music-evo/repro.lock
sudo rm /srv/shared/jobs/music-evo/repro.lock
```

## Security Notes

1. **Never commit `.env` to version control**
2. **Use strong passwords** for PostgreSQL
3. **Set up TLS** for public access (see public-access.md)
4. **Keep Docker and dependencies updated**
5. **Monitor logs** for suspicious activity
