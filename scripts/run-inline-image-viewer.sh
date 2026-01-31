#!/bin/bash
# Run the inline image scenario with the realm viewer
#
# Usage:
#   ./scripts/run-inline-image-viewer.sh
#   ./scripts/run-inline-image-viewer.sh --home   # Use home realm scenario instead

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

# Default to inline image scenario
SCENARIO="simulation/scripts/scenarios/sync_engine_inline_image.lua"

# Use home realm scenario if --home flag is passed
if [[ "$1" == "--home" ]]; then
    SCENARIO="simulation/scripts/scenarios/sync_engine_home_realm_stress.lua"
    export STRESS_LEVEL="${STRESS_LEVEL:-quick}"
fi

echo "Running scenario: $SCENARIO"
echo "Piping to realm-viewer..."

./target/debug/lua_runner "$SCENARIO" | cargo run -p indras-realm-viewer -- "$@"
