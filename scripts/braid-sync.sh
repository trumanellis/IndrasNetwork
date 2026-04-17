#!/bin/bash
# braid-sync.sh — thin client for the syncengine IPC socket.
#
# Usage: braid-sync.sh <cwd> <intent>
#
# Sends a JSON sync request to the syncengine's unix socket and prints
# the response. Intended to be called from .claude/commands/sync-braid.md.

set -euo pipefail

CWD="${1:?usage: braid-sync.sh <cwd> <intent>}"
INTENT="${2:?usage: braid-sync.sh <cwd> <intent>}"

# Resolve the socket path.
DATA_DIR="${INDRAS_DATA_DIR:-${HOME}/Library/Application Support/indras-network}"
SOCKET="${DATA_DIR}/sync.sock"

if [ ! -S "$SOCKET" ]; then
  echo "error: syncengine socket not found at $SOCKET"
  echo "Is the Synchronicity Engine app running?"
  exit 1
fi

# Build the JSON request and send via socat (preferred) or nc.
REQUEST=$(printf '{"cwd":"%s","intent":"%s"}' "$CWD" "$INTENT")

if command -v socat >/dev/null 2>&1; then
  echo "$REQUEST" | socat - UNIX-CONNECT:"$SOCKET"
elif command -v nc >/dev/null 2>&1; then
  echo "$REQUEST" | nc -U "$SOCKET"
else
  echo "error: neither socat nor nc found — install one to use braid-sync"
  exit 1
fi
