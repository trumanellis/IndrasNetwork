#!/bin/bash
# Run the home realm viewer with Lua scenario output
#
# Usage:
#   ./scripts/run-home-viewer.sh              # Quick stress level
#   ./scripts/run-home-viewer.sh -m A         # Filter to member A
#   STRESS_LEVEL=medium ./scripts/run-home-viewer.sh
#
# Environment variables:
#   STRESS_LEVEL - quick, medium, or full (default: quick)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
LOG_DIR="$ROOT_DIR/simulation/scripts/logs"

# Build both binaries first
cargo build --bin lua_runner --manifest-path "$ROOT_DIR/simulation/Cargo.toml" --quiet
cargo build -p indras-home-viewer --quiet

# Run lua_runner from simulation/scripts directory so it can find lib modules
# This writes JSONL to the logs directory
cd "$ROOT_DIR/simulation/scripts"
echo "Running sync_engine_home_realm_stress scenario (STRESS_LEVEL=${STRESS_LEVEL:-quick})..."
STRESS_LEVEL="${STRESS_LEVEL:-quick}" "$ROOT_DIR/target/debug/lua_runner" \
    --log-dir "$LOG_DIR" \
    scenarios/sync_engine_home_realm_stress.lua

# Find the generated log file and run viewer with it
LOG_FILE="$LOG_DIR/sync_engine_home_realm_stress.log"
if [ -f "$LOG_FILE" ]; then
    echo "Opening viewer with $LOG_FILE"
    "$ROOT_DIR/target/debug/indras-home-viewer" -f "$LOG_FILE" "$@"
else
    echo "Error: Log file not found at $LOG_FILE"
    exit 1
fi
