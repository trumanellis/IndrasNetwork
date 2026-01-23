#!/bin/bash
# Indras Network Docker Test Runner (Host Script)
#
# Usage:
#   ./run_tests.sh              - Build and run all tests
#   ./run_tests.sh build        - Build images only
#   ./run_tests.sh basic        - Run basic tests
#   ./run_tests.sh clean        - Clean up Docker resources
#   ./run_tests.sh logs         - View node logs

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
COMMAND=${1:-all}

cd "$SCRIPT_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step() { echo -e "${BLUE}==>${NC} $1"; }

# ============================================================================
# Commands
# ============================================================================

cmd_build() {
    log_step "Building Docker images..."
    docker-compose build
    log_info "Build complete"
}

cmd_start() {
    log_step "Starting node cluster..."
    docker-compose up -d node
    log_info "Waiting for nodes to initialize..."
    sleep 5
    docker-compose ps
}

cmd_stop() {
    log_step "Stopping node cluster..."
    docker-compose down
    log_info "Cluster stopped"
}

cmd_logs() {
    docker-compose logs -f node
}

cmd_test() {
    local scenario=${1:-all}
    log_step "Running tests: $scenario"

    # Ensure images are built
    cmd_build

    # Start the cluster
    cmd_start

    # Run tests
    docker-compose run --rm tests "$scenario"
    local exit_code=$?

    # Stop cluster
    cmd_stop

    return $exit_code
}

cmd_clean() {
    log_step "Cleaning up Docker resources..."
    docker-compose down -v --rmi local --remove-orphans
    log_info "Cleanup complete"
}

cmd_shell() {
    log_step "Opening shell in test container..."
    docker-compose run --rm --entrypoint /bin/bash tests
}

cmd_chaos() {
    log_step "Starting chaos testing..."

    # Start cluster
    cmd_start

    # Run with chaos profile
    docker-compose --profile chaos up -d chaos

    log_info "Chaos testing enabled. Use pumba commands to inject failures."
    log_info "Example: docker-compose exec chaos pumba pause -d 30s indras-docker-node-1"
}

cmd_help() {
    cat <<EOF
Indras Network Docker Test Runner

Usage: ./run_tests.sh <command> [args]

Commands:
  build           Build Docker images
  start           Start node cluster
  stop            Stop node cluster
  logs            View node logs
  test [scenario] Run tests (default: all)
  clean           Clean up Docker resources
  shell           Open shell in test container
  chaos           Start chaos testing mode
  help            Show this help

Test Scenarios:
  basic           Basic connectivity tests
  messaging       Messaging layer tests
  dtn             DTN routing tests
  partition       Network partition recovery
  crypto          Cryptographic stress tests
  integration     Full stack integration
  all             Run all test scenarios

Examples:
  ./run_tests.sh build           # Build images
  ./run_tests.sh test basic      # Run basic tests
  ./run_tests.sh test all        # Run all tests
  ./run_tests.sh logs            # Watch node logs
  ./run_tests.sh clean           # Clean up everything
EOF
}

# ============================================================================
# Main
# ============================================================================

case "$COMMAND" in
    build)
        cmd_build
        ;;
    start)
        cmd_start
        ;;
    stop)
        cmd_stop
        ;;
    logs)
        cmd_logs
        ;;
    test)
        cmd_test "${2:-all}"
        ;;
    clean)
        cmd_clean
        ;;
    shell)
        cmd_shell
        ;;
    chaos)
        cmd_chaos
        ;;
    help|--help|-h)
        cmd_help
        ;;
    all)
        cmd_test all
        ;;
    # Direct scenario shortcuts
    basic|messaging|dtn|partition|crypto|integration)
        cmd_test "$COMMAND"
        ;;
    *)
        log_error "Unknown command: $COMMAND"
        cmd_help
        exit 1
        ;;
esac
