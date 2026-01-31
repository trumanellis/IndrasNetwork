#!/bin/bash
# Run the gratitude pledge scenario and pipe to the Omni V2 viewer.
#
# Usage:
#   ./scripts/run-gratitude-pledge.sh           # default: quick level
#   STRESS_LEVEL=medium ./scripts/run-gratitude-pledge.sh
#   ./scripts/run-gratitude-pledge.sh -m A      # filter by member

STRESS_LEVEL="${STRESS_LEVEL:-quick}" cargo run --bin lua_runner --manifest-path simulation/Cargo.toml -- simulation/scripts/scenarios/sdk_gratitude_pledge.lua | cargo run -p indras-realm-viewer --bin omni-viewer-v2 -- "$@"
