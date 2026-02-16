#!/bin/bash
# Run all Lua test scenarios and report results.
# Usage: ./scripts/run-all-lua-tests.sh
#
# Exit codes: 0 = all passed, 1 = some failed

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCENARIO_DIR="$SCRIPT_DIR/scenarios"
FEATURES="--features lua-scripting"
PASS=0
FAIL=0
RESULTS=()

# Build once
echo "=== Building indras-workspace with lua-scripting feature ==="
cargo build -p indras-workspace $FEATURES 2>&1

echo ""
echo "=== Running Lua test scenarios ==="
echo ""

for scenario in "$SCENARIO_DIR"/*.lua; do
    name=$(basename "$scenario" .lua)
    echo "--- $name ---"

    if "$SCRIPT_DIR/run-lua-test.sh" "$scenario" 2>&1; then
        PASS=$((PASS + 1))
        RESULTS+=("PASS  $name")
        echo "  -> PASS"
    else
        EXIT_CODE=$?
        FAIL=$((FAIL + 1))
        case $EXIT_CODE in
            1) RESULTS+=("FAIL  $name (assertion failure)") ;;
            2) RESULTS+=("FAIL  $name (timeout)") ;;
            3) RESULTS+=("FAIL  $name (runtime error)") ;;
            *) RESULTS+=("FAIL  $name (exit code $EXIT_CODE)") ;;
        esac
        echo "  -> FAIL (exit $EXIT_CODE)"
    fi
    echo ""
done

# Summary
echo "=== Results ==="
for r in "${RESULTS[@]}"; do
    echo "  $r"
done
echo ""
echo "Total: $((PASS + FAIL))  Passed: $PASS  Failed: $FAIL"

if [ $FAIL -gt 0 ]; then
    exit 1
fi
exit 0
