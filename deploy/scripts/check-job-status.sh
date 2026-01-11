#!/bin/bash
# =============================================================================
# Check Slurm Job Status
# =============================================================================
# This script checks the status of a reproduction job.
# It can be called by the web app to poll for job completion.
#
# Usage: ./check-job-status.sh [job_id]
#        Without job_id, returns current job status
# =============================================================================

set -euo pipefail

JOBS_DIR="/srv/shared/jobs"
STATUS_FILE="${JOBS_DIR}/current_status.json"

JOB_ID="${1:-}"

if [[ -n "${JOB_ID}" ]]; then
    # Check specific job
    JOB_STATUS=$(squeue -j "${JOB_ID}" -h -o "%T" 2>/dev/null || echo "UNKNOWN")

    case "${JOB_STATUS}" in
        PENDING)
            echo '{"slurm_status": "pending"}'
            ;;
        RUNNING)
            echo '{"slurm_status": "running"}'
            ;;
        COMPLETED)
            echo '{"slurm_status": "completed"}'
            ;;
        FAILED|CANCELLED|TIMEOUT|NODE_FAIL)
            echo "{\"slurm_status\": \"failed\", \"reason\": \"${JOB_STATUS}\"}"
            ;;
        UNKNOWN|"")
            # Job not in queue - check if completed
            SACCT_STATUS=$(sacct -j "${JOB_ID}" -n -o State | head -1 | tr -d ' ')
            if [[ "${SACCT_STATUS}" == "COMPLETED" ]]; then
                echo '{"slurm_status": "completed"}'
            elif [[ -n "${SACCT_STATUS}" ]]; then
                echo "{\"slurm_status\": \"${SACCT_STATUS,,}\"}"
            else
                echo '{"slurm_status": "unknown"}'
            fi
            ;;
        *)
            echo "{\"slurm_status\": \"${JOB_STATUS,,}\"}"
            ;;
    esac
else
    # Return current status file
    if [[ -f "${STATUS_FILE}" ]]; then
        cat "${STATUS_FILE}"
    else
        echo '{"status": "idle", "message": "No active reproduction job"}'
    fi
fi
