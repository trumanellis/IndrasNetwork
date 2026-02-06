#!/bin/bash
# Usage: scripts/ship.sh [--dry-run] [--exclude branch1,branch2]
#
# Merges all completed GitButler branches into main.
# A branch is "completed" if it has 1+ commits and merges cleanly.

set -euo pipefail
cd "$(git rev-parse --toplevel)"

DRY_RUN=false
EXCLUDE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=true; shift ;;
    --exclude) EXCLUDE="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# Get applied branches that merge cleanly
BRANCHES=$(but branch list --json | python3 -c "
import sys, json
data = json.load(sys.stdin)
for stack in data.get('appliedStacks', []):
    for head in stack.get('heads', []):
        if head.get('mergesCleanly', False):
            print(head['name'])
")

MERGED=0
SKIPPED=0
MERGED_NAMES=""
SKIPPED_NAMES=""

for branch in $BRANCHES; do
  # Check exclusion list
  if echo "$EXCLUDE" | tr ',' '\n' | grep -qx "$branch"; then
    echo "SKIP (excluded): $branch"
    SKIPPED=$((SKIPPED + 1))
    SKIPPED_NAMES="$SKIPPED_NAMES  - $branch (excluded)\n"
    continue
  fi

  # Check if branch has commits
  COMMIT_COUNT=$(but show "$branch" --json | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(len(d.get('commits', [])))
")

  if [ "$COMMIT_COUNT" -eq 0 ]; then
    echo "SKIP (no commits): $branch"
    SKIPPED=$((SKIPPED + 1))
    SKIPPED_NAMES="$SKIPPED_NAMES  - $branch (no commits)\n"
    continue
  fi

  if [ "$DRY_RUN" = true ]; then
    echo "WOULD MERGE: $branch ($COMMIT_COUNT commits)"
    MERGED_NAMES="$MERGED_NAMES  - $branch ($COMMIT_COUNT commits)\n"
  else
    echo "MERGING: $branch ($COMMIT_COUNT commits)..."
    but merge "$branch"
    MERGED_NAMES="$MERGED_NAMES  - $branch ($COMMIT_COUNT commits)\n"
    sleep 1  # avoid SQLite contention
  fi
  MERGED=$((MERGED + 1))
done

echo ""
echo "=== Summary ==="
if [ $MERGED -gt 0 ]; then
  if [ "$DRY_RUN" = true ]; then
    echo "Would merge ($MERGED):"
  else
    echo "Merged ($MERGED):"
  fi
  printf "$MERGED_NAMES"
fi
if [ $SKIPPED -gt 0 ]; then
  echo "Skipped ($SKIPPED):"
  printf "$SKIPPED_NAMES"
fi
if [ "$DRY_RUN" = true ]; then
  echo ""
  echo "(dry run — nothing was actually merged)"
fi
