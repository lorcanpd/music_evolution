#!/bin/bash
#SBATCH --nodes=1
#SBATCH --ntasks=1
#SBATCH --cpus-per-task=1
#SBATCH --mem=3500M
#SBATCH --time=00:30:00
# Note: --partition, --job-name, --output, --error are set by jobrunner

# =============================================================================
# Music Evolution Reproduction Job
# =============================================================================
# This script runs on a Slurm worker node.
# It executes the reproduce binary to create the next generation.
#
# Environment variables (passed from jobrunner):
#   DATABASE_URL    - PostgreSQL connection string
#   CURRENT_GEN     - Current generation number
#   NEXT_GEN        - Next generation number to create
#   WAV_DIR         - Directory to write WAV files
# =============================================================================

set -euo pipefail

# Configuration
SHARED_DIR="${SHARED_DIR:-/srv/shared}"
STATUS_FILE="${SHARED_DIR}/jobs/music-evo/current_status.json"
APPTAINER_BIN="${APPTAINER_BIN:-/srv/shared/apptainer/bin/apptainer}"
SIF_IMAGE="${SHARED_DIR}/images/music-evo.sif"
NATIVE_BINARY="${SHARED_DIR}/music-evo/bin/reproduce"

echo "=============================================="
echo "Music Evolution Reproduction Job"
echo "=============================================="
echo "Job ID:            ${SLURM_JOB_ID:-unknown}"
echo "Node:              ${SLURMD_NODENAME:-unknown}"
echo "Current Gen:       ${CURRENT_GEN:-unset}"
echo "Next Gen:          ${NEXT_GEN:-unset}"
echo "WAV Dir:           ${WAV_DIR:-unset}"
echo "Started:           $(date -Iseconds)"
echo "=============================================="

# Validate required environment
if [[ -z "${DATABASE_URL:-}" ]]; then
    echo "ERROR: DATABASE_URL not set" >&2
    exit 1
fi
if [[ -z "${CURRENT_GEN:-}" ]] || [[ -z "${NEXT_GEN:-}" ]]; then
    echo "ERROR: CURRENT_GEN and NEXT_GEN must be set" >&2
    exit 1
fi
if [[ -z "${WAV_DIR:-}" ]]; then
    echo "ERROR: WAV_DIR not set" >&2
    exit 1
fi

# Update status to running
update_status() {
    local status="$1"
    local message="${2:-}"
    mkdir -p "$(dirname "$STATUS_FILE")"
    cat > "$STATUS_FILE" <<EOF
{
    "status": "$status",
    "message": "$message",
    "job_id": ${SLURM_JOB_ID:-null},
    "current_generation": ${CURRENT_GEN},
    "next_generation": ${NEXT_GEN},
    "timestamp": "$(date -Iseconds)",
    "node": "${SLURMD_NODENAME:-unknown}"
}
EOF
}

update_status "running" "Reproduction in progress"

# Create WAV directory
mkdir -p "$WAV_DIR"

# Run reproduction using best available method
run_reproduce() {
    local args=(--current-gen "$CURRENT_GEN" --next-gen "$NEXT_GEN" --wav-dir "$WAV_DIR")

    # Method 1: Apptainer with SIF image
    if [[ -x "$APPTAINER_BIN" ]] && [[ -f "$SIF_IMAGE" ]]; then
        echo "Running with Apptainer: $SIF_IMAGE"
        "$APPTAINER_BIN" exec \
            --bind "$SHARED_DIR:$SHARED_DIR" \
            --bind "$WAV_DIR:$WAV_DIR" \
            "$SIF_IMAGE" \
            /app/reproduce "${args[@]}"
        return $?
    fi

    # Method 2: Native binary
    if [[ -x "$NATIVE_BINARY" ]]; then
        echo "Running native binary: $NATIVE_BINARY"
        "$NATIVE_BINARY" "${args[@]}"
        return $?
    fi

    echo "ERROR: No execution method available" >&2
    echo "  - Apptainer: $APPTAINER_BIN (exists: $(test -x "$APPTAINER_BIN" && echo yes || echo no))"
    echo "  - SIF Image: $SIF_IMAGE (exists: $(test -f "$SIF_IMAGE" && echo yes || echo no))"
    echo "  - Native:    $NATIVE_BINARY (exists: $(test -x "$NATIVE_BINARY" && echo yes || echo no))"
    return 1
}

# Execute reproduction
if run_reproduce; then
    echo "=============================================="
    echo "Reproduction completed successfully"
    echo "Finished:          $(date -Iseconds)"
    echo "=============================================="

    update_status "completed" "Generation ${NEXT_GEN} created successfully"
    exit 0
else
    EXIT_CODE=$?
    echo "=============================================="
    echo "ERROR: Reproduction failed with exit code $EXIT_CODE"
    echo "Finished:          $(date -Iseconds)"
    echo "=============================================="

    update_status "failed" "Reproduction failed with exit code $EXIT_CODE"
    exit $EXIT_CODE
fi
