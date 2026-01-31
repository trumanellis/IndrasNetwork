#!/bin/bash
# List available Lua scenarios
#
# Usage:
#   ./scripts/list-scenarios.sh              # All scenarios
#   ./scripts/list-scenarios.sh sync_engine  # Filter by prefix
#   ./scripts/list-scenarios.sh discovery    # Filter by prefix

SCENARIOS_DIR="simulation/scripts/scenarios"
FILTER="${1:-}"

for f in "$SCENARIOS_DIR"/*.lua; do
    name="$(basename "$f" .lua)"
    if [ -z "$FILTER" ] || echo "$name" | grep -qi "$FILTER"; then
        echo "$name"
    fi
done
