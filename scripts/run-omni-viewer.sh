#!/bin/bash
# Run the Omni Viewer with a Lua scenario piped in
#
# Usage:
#   ./scripts/run-omni-viewer.sh                    # Default scenario
#   ./scripts/run-omni-viewer.sh -t mystic          # With theme
#   SCENARIO=sdk_proof_folder ./scripts/run-omni-viewer.sh
#   STRESS_LEVEL=full ./scripts/run-omni-viewer.sh
#
# Environment variables:
#   STRESS_LEVEL - quick, medium, or full (default: quick)
#   SCENARIO     - scenario name without .lua extension (default: sdk_quest_proof_blessing)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$ROOT_DIR"

SCENARIO="${SCENARIO:-sdk_quest_proof_blessing}"

STRESS_LEVEL="${STRESS_LEVEL:-quick}" cargo run --bin lua_runner \
    --manifest-path simulation/Cargo.toml \
    -- "simulation/scripts/scenarios/${SCENARIO}.lua" \
    | cargo run -p indras-realm-viewer --bin omni-viewer -- "$@"
