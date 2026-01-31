#!/opt/homebrew/bin/bash

# run_stress_tests.sh - Comprehensive stress test runner for IndrasNetwork
# Usage: ./run_stress_tests.sh [module|all] [stress_level] [--parallel] [--verbose] [--output-dir DIR]

set -euo pipefail

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="${PROJECT_ROOT}/target/release/lua_runner"
SCENARIOS_DIR="${SCRIPT_DIR}/scenarios"
DEFAULT_OUTPUT_DIR="${PROJECT_ROOT}/simulation/logs"
REPORTS_DIR="${PROJECT_ROOT}/simulation/reports"

# Default settings
STRESS_LEVEL="${2:-medium}"
PARALLEL=false
VERBOSE=false
OUTPUT_DIR=""

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Results tracking
declare -a PASSED_SCENARIOS=()
declare -a FAILED_SCENARIOS=()
START_TIME=$(date +%s)

# ============================================================================
# Module to Scenario Mapping
# ============================================================================

declare -A MODULE_SCENARIOS=(
    ["crypto"]="crypto_stress.lua"
    ["transport"]="transport_stress.lua"
    ["routing"]="routing_stress.lua"
    ["storage"]="storage_stress.lua"
    ["sync"]="sync_stress.lua"
    ["gossip"]="gossip_stress.lua"
    ["messaging"]="messaging_stress.lua"
    ["logging"]="logging_stress.lua"
    ["dtn"]="dtn_stress.lua"
    ["node"]="node_stress.lua"
    ["core"]="core_stress.lua"
    ["engine"]="engine_stress.lua"
    ["integration"]="integration_full_stack.lua partition_recovery.lua scalability_limit.lua"
    ["pq"]="pq_baseline_benchmark.lua pq_chaos_monkey.lua pq_concurrent_joins.lua pq_invite_stress.lua pq_large_interface_sync.lua pq_signature_throughput.lua"
    ["discovery"]="discovery_two_peer.lua discovery_peer_group.lua discovery_late_joiner.lua discovery_rate_limit.lua discovery_reconnect.lua discovery_pq_keys.lua discovery_stress.lua"
    ["sync_engine"]="sync_engine_peer_realm_stress.lua sync_engine_quest_lifecycle.lua sync_engine_contacts_stress.lua sync_engine_attention_stress.lua"
)

# ============================================================================
# Functions
# ============================================================================

print_usage() {
    cat << EOF
Usage: $0 [MODULE] [STRESS_LEVEL] [OPTIONS]

Arguments:
  MODULE          Module to test or 'all' (default: all)
                  Available modules: crypto, transport, routing, storage, sync,
                  gossip, messaging, logging, dtn, node, core, engine,
                  integration, pq, discovery, sync_engine

  STRESS_LEVEL    Test intensity level (default: medium)
                  - quick:  Fast smoke test (~1-5 min per scenario)
                  - medium: Moderate stress (~5-15 min per scenario)
                  - full:   Comprehensive stress (~15-60 min per scenario)

Options:
  --parallel      Run scenarios in parallel (uses all CPU cores)
  --verbose       Show detailed output from scenarios
  --output-dir    Custom output directory for logs (default: simulation/logs)
  --help          Show this help message

Examples:
  $0 crypto quick                    # Quick crypto stress test
  $0 pq medium --verbose             # Medium PQ tests with verbose output
  $0 all full --parallel             # Full stress test of all modules in parallel
  $0 integration medium --output-dir /tmp/stress-logs

EOF
}

log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

log_section() {
    echo
    echo -e "${CYAN}========================================${NC}"
    echo -e "${CYAN}$*${NC}"
    echo -e "${CYAN}========================================${NC}"
}

check_prerequisites() {
    log_info "Checking prerequisites..."

    # Check if lua_runner binary exists
    if [[ ! -f "$BINARY" ]]; then
        log_warn "lua_runner binary not found at $BINARY"
        log_info "Building lua_runner..."

        cd "$PROJECT_ROOT"
        if ! cargo build --release --bin lua_runner 2>&1 | grep -q "Finished"; then
            log_error "Failed to build lua_runner"
            exit 1
        fi

        if [[ ! -f "$BINARY" ]]; then
            log_error "Binary still not found after build"
            exit 1
        fi
    fi

    # Check scenarios directory
    if [[ ! -d "$SCENARIOS_DIR" ]]; then
        log_error "Scenarios directory not found: $SCENARIOS_DIR"
        exit 1
    fi

    log_success "Prerequisites check passed"
}

setup_directories() {
    log_info "Setting up output directories..."

    mkdir -p "$OUTPUT_DIR"
    mkdir -p "$REPORTS_DIR"

    log_success "Output directories ready"
    log_info "  Logs: $OUTPUT_DIR"
    log_info "  Reports: $REPORTS_DIR"
}

validate_stress_level() {
    case "$STRESS_LEVEL" in
        quick|medium|full)
            return 0
            ;;
        *)
            log_error "Invalid stress level: $STRESS_LEVEL"
            log_error "Must be one of: quick, medium, full"
            exit 1
            ;;
    esac
}

get_scenarios_for_module() {
    local module="$1"

    if [[ "$module" == "all" ]]; then
        # Return all scenarios from all modules
        local all_scenarios=()
        for mod in "${!MODULE_SCENARIOS[@]}"; do
            for scenario in ${MODULE_SCENARIOS[$mod]}; do
                all_scenarios+=("$scenario")
            done
        done
        echo "${all_scenarios[@]}" | tr ' ' '\n' | sort -u
    elif [[ -n "${MODULE_SCENARIOS[$module]:-}" ]]; then
        echo "${MODULE_SCENARIOS[$module]}"
    else
        log_error "Unknown module: $module"
        log_error "Available modules: ${!MODULE_SCENARIOS[*]}"
        exit 1
    fi
}

run_scenario() {
    local scenario="$1"
    local scenario_path="${SCENARIOS_DIR}/${scenario}"
    local scenario_name="${scenario%.lua}"
    local log_file="${OUTPUT_DIR}/${scenario_name}_${STRESS_LEVEL}.jsonl"
    local scenario_start=$(date +%s)

    if [[ ! -f "$scenario_path" ]]; then
        log_warn "Scenario file not found: $scenario_path (skipping)"
        return 0
    fi

    log_info "Running: $scenario (level: $STRESS_LEVEL)"

    # Set environment variables for the scenario
    export STRESS_LEVEL

    # Run the scenario from the scripts directory (required for Lua require paths)
    local exit_code=0
    pushd "$SCRIPT_DIR" > /dev/null
    if [[ "$VERBOSE" == true ]]; then
        "$BINARY" "scenarios/${scenario}" 2>&1 | tee "$log_file" || exit_code=$?
    else
        "$BINARY" "scenarios/${scenario}" > "$log_file" 2>&1 || exit_code=$?
    fi
    popd > /dev/null

    local scenario_end=$(date +%s)
    local scenario_duration=$((scenario_end - scenario_start))

    # Check result
    if [[ $exit_code -eq 0 ]]; then
        log_success "PASSED: $scenario (${scenario_duration}s)"
        PASSED_SCENARIOS+=("$scenario")
        return 0
    else
        log_error "FAILED: $scenario (exit code: $exit_code, duration: ${scenario_duration}s)"
        FAILED_SCENARIOS+=("$scenario")
        return 1
    fi
}

run_scenario_parallel() {
    local scenario="$1"
    # Run in background and return immediately
    run_scenario "$scenario" &
}

run_scenarios() {
    local module="$1"
    local scenarios

    log_section "Running Stress Tests for: $module"
    log_info "Stress Level: $STRESS_LEVEL"
    log_info "Parallel Mode: $PARALLEL"

    scenarios=$(get_scenarios_for_module "$module")

    if [[ -z "$scenarios" ]]; then
        log_warn "No scenarios found for module: $module"
        return 0
    fi

    local scenario_count=$(echo "$scenarios" | wc -w | tr -d ' ')
    log_info "Found $scenario_count scenario(s) to run"
    echo

    if [[ "$PARALLEL" == true ]]; then
        log_info "Running scenarios in parallel..."
        for scenario in $scenarios; do
            run_scenario_parallel "$scenario"
        done

        # Wait for all background jobs to complete
        log_info "Waiting for all scenarios to complete..."
        wait
    else
        for scenario in $scenarios; do
            run_scenario "$scenario"
            echo
        done
    fi
}

generate_summary_report() {
    local end_time=$(date +%s)
    local total_duration=$((end_time - START_TIME))
    local total_scenarios=$((${#PASSED_SCENARIOS[@]} + ${#FAILED_SCENARIOS[@]}))
    local report_file="${REPORTS_DIR}/stress_test_summary_$(date +%Y%m%d_%H%M%S).txt"

    log_section "Test Summary"

    # Generate report content
    {
        echo "IndrasNetwork Stress Test Report"
        echo "=================================="
        echo
        echo "Timestamp: $(date '+%Y-%m-%d %H:%M:%S')"
        echo "Module: ${MODULE}"
        echo "Stress Level: ${STRESS_LEVEL}"
        echo "Parallel Mode: ${PARALLEL}"
        echo
        echo "Results:"
        echo "--------"
        echo "Total Scenarios: $total_scenarios"
        echo "Passed: ${#PASSED_SCENARIOS[@]}"
        echo "Failed: ${#FAILED_SCENARIOS[@]}"
        echo "Duration: ${total_duration}s ($(date -u -r $total_duration '+%H:%M:%S' 2>/dev/null || echo "${total_duration}s"))"
        echo

        if [[ ${#PASSED_SCENARIOS[@]} -gt 0 ]]; then
            echo "Passed Scenarios:"
            for scenario in "${PASSED_SCENARIOS[@]}"; do
                echo "  ✓ $scenario"
            done
            echo
        fi

        if [[ ${#FAILED_SCENARIOS[@]} -gt 0 ]]; then
            echo "Failed Scenarios:"
            for scenario in "${FAILED_SCENARIOS[@]}"; do
                echo "  ✗ $scenario"
            done
            echo
        fi

        echo "Log Directory: $OUTPUT_DIR"
        echo "Report Location: $report_file"

    } | tee "$report_file"

    # Console summary with colors
    echo
    if [[ ${#FAILED_SCENARIOS[@]} -eq 0 ]]; then
        log_success "All tests passed! ($total_scenarios/$total_scenarios)"
    else
        log_error "Some tests failed (${#FAILED_SCENARIOS[@]}/$total_scenarios)"
        echo
        log_error "Failed scenarios:"
        for scenario in "${FAILED_SCENARIOS[@]}"; do
            echo -e "  ${RED}✗${NC} $scenario"
        done
    fi

    echo
    log_info "Total execution time: ${total_duration}s"
    log_info "Full report saved to: $report_file"

    # Return exit code based on failures
    if [[ ${#FAILED_SCENARIOS[@]} -gt 0 ]]; then
        return 1
    else
        return 0
    fi
}

# ============================================================================
# Main Script
# ============================================================================

main() {
    # Parse arguments
    MODULE="${1:-all}"

    # Show help if requested
    if [[ "$MODULE" == "--help" ]] || [[ "$MODULE" == "-h" ]]; then
        print_usage
        exit 0
    fi

    # Parse optional flags
    shift || true
    shift || true

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --parallel)
                PARALLEL=true
                shift
                ;;
            --verbose)
                VERBOSE=true
                shift
                ;;
            --output-dir)
                OUTPUT_DIR="$2"
                shift 2
                ;;
            --help|-h)
                print_usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                print_usage
                exit 1
                ;;
        esac
    done

    # Set default output dir if not specified
    if [[ -z "$OUTPUT_DIR" ]]; then
        OUTPUT_DIR="$DEFAULT_OUTPUT_DIR"
    fi

    # Validate inputs
    validate_stress_level

    # Print banner
    echo
    log_section "IndrasNetwork Stress Test Runner"
    echo

    # Run tests
    check_prerequisites
    setup_directories
    run_scenarios "$MODULE"

    # Generate summary and exit with appropriate code
    if generate_summary_report; then
        exit 0
    else
        exit 1
    fi
}

# Run main function
main "$@"
