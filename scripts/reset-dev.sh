#!/bin/bash
# reset-dev.sh - Complete reset of local development environment
#
# Usage: ./scripts/reset-dev.sh [--rebuild]
#
# Options:
#   --rebuild    Also rebuild the Docker image (slower but ensures fresh code)

set -e

cd "$(dirname "$0")/.."

echo "=== Music Evolution Development Reset ==="

# Stop containers and remove volumes
echo "Stopping containers and removing volumes..."
docker compose down -v 2>/dev/null || true

# Clean local directories
echo "Cleaning local directories..."
rm -rf audio/generations audio/current audio/.current_new
rm -rf data/greatest_hits/revisions data/greatest_hits/current data/greatest_hits/.current_new
rm -f temp_adam.wav

# Recreate directory structure
echo "Recreating directory structure..."
mkdir -p audio/generations
mkdir -p data/greatest_hits/revisions

# Start containers
if [[ "$1" == "--rebuild" ]]; then
    echo "Rebuilding and starting containers..."
    docker compose up --build -d
else
    echo "Starting containers..."
    docker compose up -d
fi

echo ""
echo "=== Reset Complete ==="
echo "Visit http://localhost:8080 to initialize the experiment"
echo ""
echo "To watch logs: docker compose logs -f app"
