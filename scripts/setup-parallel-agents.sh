#!/bin/bash
# Set up 3 parallel jj workspaces for concurrent agent development.
#
# Usage:
#   ./scripts/setup-parallel-agents.sh [name1 name2 name3]
#
# Defaults to agent1, agent2, agent3 if no names provided.
# Each workspace is rooted on main with an empty change ready for work.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# Ensure the Trunk-Pulse protocol is on main before creating workspaces
if ! jj log -r main --no-graph -T description 2>/dev/null | grep -q "Trunk-Pulse\|land-if-green\|parallel"; then
  echo "WARNING: The Trunk-Pulse scripts/hooks don't appear to be on main yet."
  echo "Land your current change to main first, then run this script."
  echo ""
fi

WS1="${1:-agent1}"
WS2="${2:-agent2}"
WS3="${3:-agent3}"

echo "Setting up parallel workspaces: $WS1, $WS2, $WS3"
echo "Base: $(jj log -r main --no-graph -T 'change_id.shortest() ++ " " ++ description.first_line()' 2>/dev/null)"
echo ""

for WS in "$WS1" "$WS2" "$WS3"; do
  DIR=".jj-workspaces/$WS"

  # Skip if workspace already exists
  if jj workspace list 2>/dev/null | grep -q "^$WS:"; then
    echo "  $WS: already exists, skipping"
    continue
  fi

  # Clean up orphaned directory if present
  [ -d "$DIR" ] && rm -rf "$DIR"

  jj workspace add "$DIR" --revision main

  # Symlink .claude/ so hooks and settings propagate to workspaces
  if [ ! -e "$DIR/.claude" ] && [ -d ".claude" ]; then
    ln -s "$(pwd)/.claude" "$DIR/.claude"
  fi

  echo "  $WS: created at $DIR"
done

echo ""
echo "Workspaces ready:"
jj workspace list
echo ""
echo "To launch Claude Code in each workspace:"
for WS in "$WS1" "$WS2" "$WS3"; do
  echo "  cd $(pwd)/.jj-workspaces/$WS && claude"
done
echo ""
echo "Each agent should assign itself a feature area and start working."
echo "The land-if-green hook will auto-merge to main when full tests pass."
