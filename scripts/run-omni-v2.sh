#!/bin/bash
# Run the Omni V2 Viewer (calm observation dashboard)
#
# Usage:
#   ./scripts/run-omni-v2.sh                              # Scenario picker UI (default)
#   SCENARIO=sync_engine_harmony_proof ./scripts/run-omni-v2.sh   # Pipe a specific scenario
#   STRESS_LEVEL=full SCENARIO=sync_engine_stress ./scripts/run-omni-v2.sh
#   ./scripts/run-omni-v2.sh -t light                     # Picker with light theme
#
# Environment variables:
#   STRESS_LEVEL - quick, medium, or full (default: quick)
#   SCENARIO     - scenario name without .lua extension (if set, pipes that scenario)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$ROOT_DIR"

# If SCENARIO is set, pipe it into the viewer; otherwise launch the picker
if [ -n "$SCENARIO" ]; then
    STRESS_LEVEL="${STRESS_LEVEL:-quick}" cargo run --bin lua_runner \
        --manifest-path simulation/Cargo.toml \
        -- "simulation/scripts/scenarios/${SCENARIO}.lua" \
        | cargo run -p indras-realm-viewer --bin omni-viewer-v2 -- "$@"
else
    cargo run -p indras-realm-viewer --bin omni-viewer-v2 -- "$@"
fi
