#!/bin/bash
# Run the Live ABC Relay scenario — store-and-forward test
#
# Tests that messages sent while a peer is offline are delivered
# when it comes back online via CRDT sync.
#
# Usage:
#   ./scripts/run-live-abc-relay.sh              # normal run
#   ./scripts/run-live-abc-relay.sh --debug       # with sync debug logs
#   ./scripts/run-live-abc-relay.sh | cargo run -p indras-realm-viewer --bin omni-viewer-v2

if [[ "$1" == "--debug" ]]; then
  RUST_LOG="warn,indras_node::sync_task=debug,indras_node::message_handler=debug,indras_simulation=info"
  shift
else
  RUST_LOG="${RUST_LOG:-warn}"
fi

RUST_LOG="$RUST_LOG" cargo run --bin lua_runner \
  --manifest-path simulation/Cargo.toml \
  -- simulation/scripts/scenarios/live_abc_relay.lua "$@"
