#!/bin/bash
# Run the Harmony Proof scenario through the Omni V2 viewer
#
# Usage:
#   ./scripts/run-harmony.sh          # Run with default settings
#   ./scripts/run-harmony.sh -m A     # Run with member filter
#
# Environment:
#   STRESS_LEVEL  quick|medium|full (default: quick)

STRESS_LEVEL="${STRESS_LEVEL:-quick}" cargo run --bin lua_runner \
    --manifest-path simulation/Cargo.toml \
    -- "simulation/scripts/scenarios/sync_engine_harmony_proof.lua" \
    | cargo run -p indras-realm-viewer --bin omni-viewer-v2 -- "$@"
