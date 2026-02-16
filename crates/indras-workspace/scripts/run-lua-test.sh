#!/bin/bash
# Run a Lua test scenario against indras-workspace.
# Usage: ./scripts/run-lua-test.sh scripts/scenarios/two_peer_chat.lua
#
# For multi-instance tests, launches Joy and Love instances in parallel.
# For single-instance tests, launches one instance.

set -e

SCRIPT="${1:?Usage: run-lua-test.sh <script.lua>}"
FEATURES="--features lua-scripting"

# Build once
echo "Building indras-workspace with lua-scripting feature..."
cargo build -p indras-workspace $FEATURES 2>&1

# Check if this is a multi-instance scenario (contains "my_name")
if grep -q "my_name" "$SCRIPT" 2>/dev/null; then
    echo "Multi-instance scenario detected — launching Joy and Love"

    INDRAS_NAME=Joy INDRAS_SCRIPT="$SCRIPT" \
      INDRAS_WIN_X=100 INDRAS_WIN_Y=100 \
      cargo run -p indras-workspace $FEATURES -- --clean --script="$SCRIPT" &
    PID_JOY=$!

    INDRAS_NAME=Love INDRAS_SCRIPT="$SCRIPT" \
      INDRAS_WIN_X=700 INDRAS_WIN_Y=100 \
      cargo run -p indras-workspace $FEATURES -- --clean --script="$SCRIPT" &
    PID_LOVE=$!

    wait $PID_JOY
    EXIT_JOY=$?
    wait $PID_LOVE
    EXIT_LOVE=$?

    if [ $EXIT_JOY -eq 0 ] && [ $EXIT_LOVE -eq 0 ]; then
        echo "ALL TESTS PASSED"
        exit 0
    else
        echo "TESTS FAILED (Joy=$EXIT_JOY, Love=$EXIT_LOVE)"
        exit 1
    fi
else
    echo "Single-instance scenario — launching one instance"
    cargo run -p indras-workspace $FEATURES -- --script="$SCRIPT"
fi
