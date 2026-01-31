#!/bin/bash
# Run the immune response simulation through the omni viewer
#
# Demonstrates the full sentiment/trust/blocking lifecycle:
# detection → signal propagation → graduated response → cascade → recovery
#
# Usage:
#   ./scripts/run-immune-sim.sh                    # Default (quick)
#   ./scripts/run-immune-sim.sh -t mystic          # With theme
#   STRESS_LEVEL=full ./scripts/run-immune-sim.sh  # Full simulation
#
# Environment variables:
#   STRESS_LEVEL - quick, medium, or full (default: quick)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$ROOT_DIR"

STRESS_LEVEL="${STRESS_LEVEL:-quick}" cargo run --bin lua_runner \
    --manifest-path simulation/Cargo.toml \
    -- "simulation/scripts/scenarios/sync_engine_immune_response.lua" \
    | cargo run -p indras-realm-viewer --bin omni-viewer-v2 -- "$@"
