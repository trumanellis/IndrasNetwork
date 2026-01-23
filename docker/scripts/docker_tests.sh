#!/bin/bash
# Indras Network Docker Test Runner
#
# Usage:
#   ./docker_tests.sh basic      - Run basic connectivity tests
#   ./docker_tests.sh messaging  - Run messaging tests
#   ./docker_tests.sh partition  - Run network partition recovery tests
#   ./docker_tests.sh all        - Run all tests

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCENARIO=${1:-basic}
RESULTS_DIR=${RESULTS_DIR:-/app/results}

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Ensure results directory exists
mkdir -p "$RESULTS_DIR"

# ============================================================================
# Test Functions
# ============================================================================

test_basic_connectivity() {
    log_info "Running basic connectivity tests..."

    # Run the basic scenario
    if lua_runner /app/scripts/scenarios/node_stress.lua 2>&1 | tee "$RESULTS_DIR/basic.log"; then
        log_info "Basic connectivity test: PASSED"
        return 0
    else
        log_error "Basic connectivity test: FAILED"
        return 1
    fi
}

test_messaging() {
    log_info "Running messaging tests..."

    # Run the messaging scenario
    if lua_runner /app/scripts/scenarios/messaging_stress.lua 2>&1 | tee "$RESULTS_DIR/messaging.log"; then
        log_info "Messaging test: PASSED"
        return 0
    else
        log_error "Messaging test: FAILED"
        return 1
    fi
}

test_dtn_operations() {
    log_info "Running DTN operations tests..."

    # Run the DTN scenario
    if lua_runner /app/scripts/scenarios/dtn_stress.lua 2>&1 | tee "$RESULTS_DIR/dtn.log"; then
        log_info "DTN operations test: PASSED"
        return 0
    else
        log_error "DTN operations test: FAILED"
        return 1
    fi
}

test_partition_recovery() {
    log_info "Running partition recovery tests..."

    # Run the partition recovery scenario
    if lua_runner /app/scripts/scenarios/partition_recovery.lua 2>&1 | tee "$RESULTS_DIR/partition.log"; then
        log_info "Partition recovery test: PASSED"
        return 0
    else
        log_error "Partition recovery test: FAILED"
        return 1
    fi
}

test_crypto_stress() {
    log_info "Running crypto stress tests..."

    # Run the crypto scenario
    if lua_runner /app/scripts/scenarios/crypto_stress.lua 2>&1 | tee "$RESULTS_DIR/crypto.log"; then
        log_info "Crypto stress test: PASSED"
        return 0
    else
        log_error "Crypto stress test: FAILED"
        return 1
    fi
}

test_full_integration() {
    log_info "Running full integration tests..."

    # Run the full integration scenario
    if lua_runner /app/scripts/scenarios/integration_full_stack.lua 2>&1 | tee "$RESULTS_DIR/integration.log"; then
        log_info "Full integration test: PASSED"
        return 0
    else
        log_error "Full integration test: FAILED"
        return 1
    fi
}

# ============================================================================
# Main
# ============================================================================

main() {
    log_info "Starting Indras Network Docker Tests"
    log_info "Scenario: $SCENARIO"
    log_info "Results directory: $RESULTS_DIR"

    local failed=0

    case "$SCENARIO" in
        basic)
            test_basic_connectivity || failed=$((failed + 1))
            ;;
        messaging)
            test_messaging || failed=$((failed + 1))
            ;;
        dtn)
            test_dtn_operations || failed=$((failed + 1))
            ;;
        partition)
            test_partition_recovery || failed=$((failed + 1))
            ;;
        crypto)
            test_crypto_stress || failed=$((failed + 1))
            ;;
        integration)
            test_full_integration || failed=$((failed + 1))
            ;;
        all)
            test_basic_connectivity || failed=$((failed + 1))
            test_messaging || failed=$((failed + 1))
            test_dtn_operations || failed=$((failed + 1))
            test_crypto_stress || failed=$((failed + 1))
            test_partition_recovery || failed=$((failed + 1))
            test_full_integration || failed=$((failed + 1))
            ;;
        *)
            log_error "Unknown scenario: $SCENARIO"
            log_info "Available scenarios: basic, messaging, dtn, partition, crypto, integration, all"
            exit 1
            ;;
    esac

    # Write summary
    if [ $failed -eq 0 ]; then
        log_info "All tests PASSED"
        echo "PASSED" > "$RESULTS_DIR/summary.txt"
        exit 0
    else
        log_error "$failed test(s) FAILED"
        echo "FAILED: $failed tests" > "$RESULTS_DIR/summary.txt"
        exit 1
    fi
}

main "$@"
