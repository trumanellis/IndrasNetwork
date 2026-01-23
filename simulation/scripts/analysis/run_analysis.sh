#!/bin/bash
#
# PQ Crypto Stress Test Runner
#
# Runs all PQ stress test scenarios and analyzes the results.
# Outputs a comprehensive report with pass/fail status for each scenario.
#
# Usage:
#   ./run_analysis.sh                  # Run all scenarios
#   ./run_analysis.sh --scenario pq_baseline_benchmark  # Run specific scenario
#   ./run_analysis.sh --quick          # Quick mode (fewer iterations)
#

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SIMULATION_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKSPACE_DIR="$(cd "$SIMULATION_DIR/.." && pwd)"
SCENARIOS_DIR="$SCRIPT_DIR/../scenarios"
REPORTS_DIR="$SIMULATION_DIR/reports"
LOGS_DIR="$SIMULATION_DIR/logs"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Scenarios to run
PQ_SCENARIOS=(
    "pq_baseline_benchmark"
    "pq_signature_throughput"
    "pq_concurrent_joins"
    "pq_large_interface_sync"
    "pq_invite_stress"
    "pq_chaos_monkey"
)

# Parse arguments
SELECTED_SCENARIO=""
QUICK_MODE=false
VERBOSE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --scenario|-s)
            SELECTED_SCENARIO="$2"
            shift 2
            ;;
        --quick|-q)
            QUICK_MODE=true
            shift
            ;;
        --verbose|-v)
            VERBOSE=true
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --scenario, -s NAME    Run only the specified scenario"
            echo "  --quick, -q            Quick mode (fewer iterations)"
            echo "  --verbose, -v          Verbose output"
            echo "  --help, -h             Show this help"
            echo ""
            echo "Available scenarios:"
            for scenario in "${PQ_SCENARIOS[@]}"; do
                echo "  - $scenario"
            done
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Create directories
mkdir -p "$REPORTS_DIR" "$LOGS_DIR"

# Build the simulation
echo -e "${BLUE}Building simulation...${NC}"
cd "$WORKSPACE_DIR"
cargo build --release --bin lua_runner 2>&1 | tail -5

# Check if lua_runner exists (workspace builds to root target directory)
LUA_RUNNER="$WORKSPACE_DIR/target/release/lua_runner"
if [[ ! -f "$LUA_RUNNER" ]]; then
    LUA_RUNNER="$WORKSPACE_DIR/target/debug/lua_runner"
fi

if [[ ! -f "$LUA_RUNNER" ]]; then
    echo -e "${RED}Error: lua_runner binary not found${NC}"
    echo "Run 'cargo build --release' in the workspace root first."
    exit 1
fi

# Determine which scenarios to run
if [[ -n "$SELECTED_SCENARIO" ]]; then
    SCENARIOS_TO_RUN=("$SELECTED_SCENARIO")
else
    SCENARIOS_TO_RUN=("${PQ_SCENARIOS[@]}")
fi

# Track results (avoid bash 4+ associative arrays for compatibility)
TOTAL_SCENARIOS=0
PASSED_SCENARIOS=0
FAILED_SCENARIOS=0
PASSED_LIST=""
FAILED_LIST=""

echo ""
echo -e "${BLUE}Running PQ stress tests...${NC}"
echo "==========================================="

# Run from simulation directory for correct relative paths
cd "$SIMULATION_DIR"

for scenario in "${SCENARIOS_TO_RUN[@]}"; do
    SCENARIO_FILE="$SCENARIOS_DIR/${scenario}.lua"

    if [[ ! -f "$SCENARIO_FILE" ]]; then
        echo -e "${YELLOW}Warning: Scenario file not found: $SCENARIO_FILE${NC}"
        continue
    fi

    TOTAL_SCENARIOS=$((TOTAL_SCENARIOS + 1))
    LOG_FILE="$LOGS_DIR/${scenario}.log"
    START_TIME=$(date +%s)

    printf "[%d/%d] %-35s " "$TOTAL_SCENARIOS" "${#SCENARIOS_TO_RUN[@]}" "$scenario"

    # Run scenario with logging (use relative path from simulation dir)
    RELATIVE_SCENARIO="scripts/scenarios/${scenario}.lua"
    if $VERBOSE; then
        "$LUA_RUNNER" --level debug "$RELATIVE_SCENARIO" 2>&1 | tee "$LOG_FILE"
        EXIT_CODE=${PIPESTATUS[0]}
    else
        "$LUA_RUNNER" --level info "$RELATIVE_SCENARIO" > "$LOG_FILE" 2>&1
        EXIT_CODE=$?
    fi

    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))

    if [[ $EXIT_CODE -eq 0 ]]; then
        echo -e "${GREEN}PASS${NC} (${DURATION}s)"
        PASSED_LIST="$PASSED_LIST $scenario"
        PASSED_SCENARIOS=$((PASSED_SCENARIOS + 1))
    else
        echo -e "${RED}FAIL${NC} (${DURATION}s)"
        FAILED_LIST="$FAILED_LIST $scenario"
        FAILED_SCENARIOS=$((FAILED_SCENARIOS + 1))

        if $VERBOSE; then
            echo "  Exit code: $EXIT_CODE"
            echo "  Log file: $LOG_FILE"
        fi
    fi
done

echo ""
echo "==========================================="

# Run analysis on all logs
echo -e "${BLUE}Analyzing results...${NC}"

COMBINED_REPORT="$REPORTS_DIR/pq_stress_report.json"

# Create combined report
python3 "$SCRIPT_DIR/pq_analyzer.py" \
    --input "$LOGS_DIR/pq_*.log" \
    --output "$COMBINED_REPORT" \
    ${VERBOSE:+--verbose}

ANALYZER_EXIT=$?

echo ""
echo "==========================================="
echo -e "${BLUE}Summary${NC}"
echo "==========================================="
echo ""

# Print individual scenario results
for scenario in "${SCENARIOS_TO_RUN[@]}"; do
    if [[ " $PASSED_LIST " == *" $scenario "* ]]; then
        echo -e "  ${GREEN}✓${NC} $scenario"
    elif [[ " $FAILED_LIST " == *" $scenario "* ]]; then
        echo -e "  ${RED}✗${NC} $scenario"
    else
        echo -e "  ${YELLOW}?${NC} $scenario"
    fi
done

echo ""
echo "Scenarios: $PASSED_SCENARIOS passed, $FAILED_SCENARIOS failed, $TOTAL_SCENARIOS total"
echo ""
echo "Report generated: $COMBINED_REPORT"

# Print key metrics if report exists
if [[ -f "$COMBINED_REPORT" ]]; then
    echo ""
    echo "Key metrics:"
    python3 -c "
import json
import sys

try:
    with open('$COMBINED_REPORT', 'r') as f:
        report = json.load(f)

    metrics = report.get('metrics', {})

    sig = metrics.get('signature', {})
    if sig.get('latency', {}).get('count', 0) > 0:
        latency = sig.get('latency', {})
        print(f\"  Signature throughput: {int(latency.get('ops_per_sec', 0)):,} ops/sec\")
        print(f\"  Signature p99 latency: {latency.get('p99_us', 0)}us\")

    kem = metrics.get('kem', {})
    if kem.get('encap_latency', {}).get('count', 0) > 0:
        encap = kem.get('encap_latency', {})
        print(f\"  KEM throughput: {int(encap.get('ops_per_sec', 0)):,} ops/sec\")
        print(f\"  KEM p99 latency: {encap.get('p99_us', 0)}us\")

    thresholds = report.get('thresholds', {})
    print(f\"  Threshold checks: {thresholds.get('passed', 0)}/{thresholds.get('total', 0)} passed\")
except Exception as e:
    print(f\"  (Unable to read metrics: {e})\", file=sys.stderr)
"
fi

echo ""

# Exit with failure if any scenarios failed
if [[ $FAILED_SCENARIOS -gt 0 ]] || [[ $ANALYZER_EXIT -ne 0 ]]; then
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
else
    echo -e "${GREEN}All thresholds passed.${NC}"
    exit 0
fi
