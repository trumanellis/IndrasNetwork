#!/bin/bash
# Run the realm viewer with live Lua scenario output
#
# Usage:
#   ./scripts/run-realm-viewer.sh                    # Default scenario
#   ./scripts/run-realm-viewer.sh -t mystic          # With theme
#   SCENARIO=sdk_realm_basic ./scripts/run-realm-viewer.sh
#
# Environment variables:
#   STRESS_LEVEL - quick, medium, or full (default: quick)
#   SCENARIO     - scenario name without .lua extension (default: sdk_realm_stress)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$ROOT_DIR"

SCENARIO="${SCENARIO:-sdk_realm_stress}"

STRESS_LEVEL="${STRESS_LEVEL:-quick}" cargo run --bin lua_runner \
    --manifest-path simulation/Cargo.toml \
    -- "simulation/scripts/scenarios/${SCENARIO}.lua" \
    | cargo run -p indras-realm-viewer -- "$@"
