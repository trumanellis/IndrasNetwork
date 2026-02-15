#!/bin/bash
# Run the live Harmony Proof scenario
#
# Three real IndrasNode instances (Love, Joy, Peace) exchange messages
# over actual QUIC transport, demonstrating the full quest/proof/blessing/token
# lifecycle on real P2P infrastructure.
#
# Usage:
#   ./scripts/run-live-harmony.sh                    # CLI output
#   ./scripts/run-live-harmony.sh --viewer            # pipe to omni-viewer-v2
#   RUST_LOG=debug ./scripts/run-live-harmony.sh

cargo run --bin lua_runner --manifest-path simulation/Cargo.toml \
  -- simulation/scripts/scenarios/live_harmony.lua "$@"
