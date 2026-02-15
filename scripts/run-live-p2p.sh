#!/bin/bash
# Run the live P2P sync scenario
#
# This spawns two real IndrasNode instances and tests actual P2P sync.
#
# Usage:
#   ./scripts/run-live-p2p.sh
#   RUST_LOG=debug ./scripts/run-live-p2p.sh

cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
  -- simulation/scripts/scenarios/live_p2p_sync.lua "$@"
