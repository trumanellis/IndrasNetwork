#!/bin/bash
# Land the current jj change to main and push.
# Usage: ./scripts/jj-land.sh
#
# Rebases onto main, advances the bookmark, and pushes.
# If push fails (another agent landed first), fetches and retries.

set -euo pipefail

jj rebase -d main
jj bookmark set main -r @

if ! jj git push 2>/dev/null; then
  echo "Push failed — fetching and retrying..."
  jj git fetch
  jj rebase -d main
  jj bookmark set main -r @
  jj git push
fi
