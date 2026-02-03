#!/bin/bash
# Run the fractal canvas with live Lua scenario output
#
# Usage:
#   ./scripts/run-fractal-canvas.sh                    # Default scenario
#   SCENARIO=sync_engine_quest_lifecycle ./scripts/run-fractal-canvas.sh
#   SCENARIO=sync_engine_stress ./scripts/run-fractal-canvas.sh
#
# Environment variables:
#   STRESS_LEVEL - quick, medium, or full (default: quick)
#   SCENARIO     - scenario name without .lua extension (default: sync_engine_quest_proof_blessing)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$ROOT_DIR"

SCENARIO="${SCENARIO:-sync_engine_quest_proof_blessing}"

STRESS_LEVEL="${STRESS_LEVEL:-quick}" cargo run --bin lua_runner \
    --manifest-path simulation/Cargo.toml \
    -- "simulation/scripts/scenarios/${SCENARIO}.lua" \
    | cargo run -p indras-fractal-canvas -- --replay "$@"
