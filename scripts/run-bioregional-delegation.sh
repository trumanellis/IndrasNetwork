#!/bin/bash
# Run the bioregional delegation scenario and verify output
#
# Usage:
#   ./scripts/run-bioregional-delegation.sh           # run + verify
#   ./scripts/run-bioregional-delegation.sh --no-check # run only
#
# Environment variables:
#   STRESS_LEVEL - quick, medium, or full (default: quick)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
LOG_DIR="$ROOT_DIR/simulation/scripts/logs"
LOG_FILE="$LOG_DIR/sync_engine_bioregional_delegation.log"
CHECK=true

for arg in "$@"; do
    if [ "$arg" = "--no-check" ]; then
        CHECK=false
        shift
        break
    fi
done

# Build lua_runner
cargo build --bin lua_runner --manifest-path "$ROOT_DIR/simulation/Cargo.toml" --quiet

# Run from simulation/scripts so lib modules resolve
cd "$ROOT_DIR/simulation/scripts"
echo "Running bioregional delegation scenario (STRESS_LEVEL=${STRESS_LEVEL:-quick})..."
STRESS_LEVEL="${STRESS_LEVEL:-quick}" "$ROOT_DIR/target/debug/lua_runner" \
    --log-dir "$LOG_DIR" \
    scenarios/sync_engine_bioregional_delegation.lua "$@"

echo "Done. Log at: $LOG_FILE"

if [ "$CHECK" = false ]; then
    exit 0
fi

# ── Automated verification ────────────────────────────────────────────────
echo ""
echo "Verifying scenario output..."
PASS=0
FAIL=0

check() {
    local desc="$1"
    local pattern="$2"
    local expected="$3"
    local actual
    actual=$(grep -c "$pattern" "$LOG_FILE" 2>/dev/null || true)
    actual="${actual:-0}"
    if [ "$actual" -ge "$expected" ]; then
        echo "  ok  $desc ($actual >= $expected)"
        PASS=$((PASS + 1))
    else
        echo "  FAIL $desc (got $actual, expected >= $expected)"
        FAIL=$((FAIL + 1))
    fi
}

# Act 1: Delegation hierarchy
check "Delegation events issued"       'delegation_issued'          7
check "Realm delegations"              'level.*Realm'               2
check "Subrealm delegations"           'level.*Subrealm'           2
check "Bioregion delegation"           'level.*Bioregion'          1
check "Ecoregion delegation"           'level.*Ecoregion'          1
check "Individual delegation"          'level.*Individual'         1

# Act 2: Attestation & validation
check "Humanness attestations"         'humanness_attestation'     2
check "Chain validations (valid)"      'chain_validated'           2
check "Humanness freshness"            'humanness_freshness'       1

# Act 3 & 4 & 5: Trust evaluations
check "Trust evaluations"              'temple_trust_evaluation'   5
check "Strong verdicts"                'verdict.*strong'           2
check "Weak verdict"                   'verdict.*weak'             1
check "Degraded verdict"               'verdict.*degraded'         1
check "Moderate verdict"               'verdict.*moderate'         1

# Narrative structure
check "All 5 acts + epilogue logged"   'Act [1-5]\|Epilogue'       6
check "Sentiment adjustments"          'sentiment_set'            16
check "Chat messages"                  'chat_message'             15

echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "All $PASS checks passed."
else
    echo "$PASS passed, $FAIL FAILED."
    exit 1
fi
