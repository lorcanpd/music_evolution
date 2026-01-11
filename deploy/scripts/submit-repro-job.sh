#!/bin/bash
# =============================================================================
# Slurm Job Submission Script for Music Evolution Reproduction
# =============================================================================
# This script runs ON THE HOST (martin), not inside the container.
# It submits a reproduction job to the Slurm cluster.
#
# Usage: ./submit-repro-job.sh <current_gen> <next_gen> <database_url>
#
# The script:
# 1. Acquires a lock to ensure single-flight reproduction
# 2. Submits an sbatch job to the "debug" partition
# 3. Writes job ID to a status file for the web app to poll
# =============================================================================

set -euo pipefail

# Configuration
SHARED_DIR="/srv/shared"
JOBS_DIR="${SHARED_DIR}/jobs"
LOCK_FILE="${JOBS_DIR}/repro.lock"
APPTAINER_BIN="${SHARED_DIR}/apptainer/bin/apptainer"
SIF_IMAGE="${SHARED_DIR}/images/music-evo.sif"
LOG_DIR="${JOBS_DIR}/logs"

# Arguments
CURRENT_GEN="${1:-}"
NEXT_GEN="${2:-}"
DATABASE_URL="${3:-}"

# Validate arguments
if [[ -z "$CURRENT_GEN" || -z "$NEXT_GEN" || -z "$DATABASE_URL" ]]; then
    echo "Usage: $0 <current_gen> <next_gen> <database_url>" >&2
    exit 1
fi

# Ensure directories exist
mkdir -p "${JOBS_DIR}" "${LOG_DIR}"

# =============================================================================
# Acquire Lock (single-flight reproduction)
# =============================================================================
acquire_lock() {
    exec 200>"${LOCK_FILE}"
    if ! flock -n 200; then
        echo '{"status": "error", "message": "Another reproduction job is already running"}' > "${JOBS_DIR}/current_status.json"
        exit 1
    fi
    # Lock acquired - write PID
    echo $$ > "${LOCK_FILE}"
}

release_lock() {
    rm -f "${LOCK_FILE}"
}

trap release_lock EXIT

acquire_lock

# =============================================================================
# Create Job Status File
# =============================================================================
JOB_TIMESTAMP=$(date +%Y%m%d_%H%M%S)
JOB_STATUS_FILE="${JOBS_DIR}/repro_${JOB_TIMESTAMP}.json"

cat > "${JOB_STATUS_FILE}" <<EOF
{
    "status": "submitting",
    "current_generation": ${CURRENT_GEN},
    "next_generation": ${NEXT_GEN},
    "timestamp": "$(date -Iseconds)",
    "job_id": null
}
EOF

# Create symlink to current job
ln -sf "$(basename "${JOB_STATUS_FILE}")" "${JOBS_DIR}/current_status.json"

# =============================================================================
# Create Slurm Job Script
# =============================================================================
JOB_SCRIPT="${JOBS_DIR}/repro_${JOB_TIMESTAMP}.sbatch"

cat > "${JOB_SCRIPT}" <<'SBATCH_EOF'
#!/bin/bash
#SBATCH --job-name=music-evo-repro
#SBATCH --partition=debug
#SBATCH --nodes=1
#SBATCH --ntasks=1
#SBATCH --cpus-per-task=4
#SBATCH --mem=4G
#SBATCH --time=00:30:00
#SBATCH --output=JOBS_DIR/logs/repro_%j.out
#SBATCH --error=JOBS_DIR/logs/repro_%j.err

set -euo pipefail

# Variables will be substituted
CURRENT_GEN=__CURRENT_GEN__
NEXT_GEN=__NEXT_GEN__
DATABASE_URL="__DATABASE_URL__"
STATUS_FILE="__STATUS_FILE__"
APPTAINER_BIN="__APPTAINER_BIN__"
SIF_IMAGE="__SIF_IMAGE__"
OUTPUT_DIR="/srv/shared/music-evo/current_generation"

echo "Starting reproduction: generation ${CURRENT_GEN} -> ${NEXT_GEN}"
echo "Job ID: ${SLURM_JOB_ID}"

# Update status to running
cat > "${STATUS_FILE}" <<EOF
{
    "status": "running",
    "current_generation": ${CURRENT_GEN},
    "next_generation": ${NEXT_GEN},
    "timestamp": "$(date -Iseconds)",
    "job_id": "${SLURM_JOB_ID}"
}
EOF

# Run reproduction using Apptainer
# The reproduction binary reads from DATABASE_URL and writes WAVs to current_generation/
export DATABASE_URL

if [[ -f "${SIF_IMAGE}" ]]; then
    echo "Running with Apptainer..."
    "${APPTAINER_BIN}" exec \
        --bind /srv/shared:/srv/shared \
        --bind "${OUTPUT_DIR}:/app/current_generation" \
        "${SIF_IMAGE}" \
        /app/reproduce --current-gen "${CURRENT_GEN}" --next-gen "${NEXT_GEN}"
else
    echo "SIF image not found, attempting native binary..."
    # Fallback: Run native binary if available
    /srv/shared/music-evo/bin/reproduce \
        --current-gen "${CURRENT_GEN}" \
        --next-gen "${NEXT_GEN}"
fi

EXIT_CODE=$?

# Update status based on result
if [[ ${EXIT_CODE} -eq 0 ]]; then
    cat > "${STATUS_FILE}" <<EOF
{
    "status": "completed",
    "current_generation": ${CURRENT_GEN},
    "next_generation": ${NEXT_GEN},
    "timestamp": "$(date -Iseconds)",
    "job_id": "${SLURM_JOB_ID}"
}
EOF
    echo "Reproduction completed successfully"
else
    cat > "${STATUS_FILE}" <<EOF
{
    "status": "failed",
    "current_generation": ${CURRENT_GEN},
    "next_generation": ${NEXT_GEN},
    "timestamp": "$(date -Iseconds)",
    "job_id": "${SLURM_JOB_ID}",
    "exit_code": ${EXIT_CODE}
}
EOF
    echo "Reproduction failed with exit code ${EXIT_CODE}"
fi

exit ${EXIT_CODE}
SBATCH_EOF

# Substitute variables in job script
sed -i "s|JOBS_DIR|${JOBS_DIR}|g" "${JOB_SCRIPT}"
sed -i "s|__CURRENT_GEN__|${CURRENT_GEN}|g" "${JOB_SCRIPT}"
sed -i "s|__NEXT_GEN__|${NEXT_GEN}|g" "${JOB_SCRIPT}"
sed -i "s|__DATABASE_URL__|${DATABASE_URL}|g" "${JOB_SCRIPT}"
sed -i "s|__STATUS_FILE__|${JOB_STATUS_FILE}|g" "${JOB_SCRIPT}"
sed -i "s|__APPTAINER_BIN__|${APPTAINER_BIN}|g" "${JOB_SCRIPT}"
sed -i "s|__SIF_IMAGE__|${SIF_IMAGE}|g" "${JOB_SCRIPT}"

# =============================================================================
# Submit Job
# =============================================================================
echo "Submitting Slurm job..."
JOB_OUTPUT=$(sbatch "${JOB_SCRIPT}" 2>&1)
SBATCH_EXIT=$?

if [[ ${SBATCH_EXIT} -ne 0 ]]; then
    cat > "${JOB_STATUS_FILE}" <<EOF
{
    "status": "error",
    "current_generation": ${CURRENT_GEN},
    "next_generation": ${NEXT_GEN},
    "timestamp": "$(date -Iseconds)",
    "message": "Failed to submit job: ${JOB_OUTPUT}"
}
EOF
    echo "Failed to submit job: ${JOB_OUTPUT}" >&2
    exit 1
fi

# Extract job ID from sbatch output (format: "Submitted batch job 12345")
JOB_ID=$(echo "${JOB_OUTPUT}" | grep -oP 'Submitted batch job \K\d+')

if [[ -z "${JOB_ID}" ]]; then
    cat > "${JOB_STATUS_FILE}" <<EOF
{
    "status": "error",
    "current_generation": ${CURRENT_GEN},
    "next_generation": ${NEXT_GEN},
    "timestamp": "$(date -Iseconds)",
    "message": "Could not parse job ID from: ${JOB_OUTPUT}"
}
EOF
    exit 1
fi

# Update status with job ID
cat > "${JOB_STATUS_FILE}" <<EOF
{
    "status": "queued",
    "current_generation": ${CURRENT_GEN},
    "next_generation": ${NEXT_GEN},
    "timestamp": "$(date -Iseconds)",
    "job_id": "${JOB_ID}"
}
EOF

echo "Job submitted successfully: ${JOB_ID}"
echo "${JOB_ID}"
