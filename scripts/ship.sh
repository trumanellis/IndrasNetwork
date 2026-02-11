#!/bin/bash
# Usage: scripts/ship.sh [--dry-run] [--exclude branch1,branch2]
#
# Merges all completed GitButler branches into main.
# A branch is "completed" if it has 1+ commits and merges cleanly.
#
# Works with both gb-local and origin targets:
#   - gb-local: uses `but merge`
#   - origin:   merges branch tip into local main via git, then pushes

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

DRY_RUN=false
EXCLUDE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=true; shift ;;
    --exclude) EXCLUDE="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# Detect target remote
TARGET_BRANCH=$(but config target --json | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(d['branch'])
")
TARGET_REMOTE=$(echo "$TARGET_BRANCH" | cut -d/ -f1)
LOCAL_BRANCH=$(echo "$TARGET_BRANCH" | cut -d/ -f2-)

echo "Target: $TARGET_BRANCH (remote: $TARGET_REMOTE, branch: $LOCAL_BRANCH)"

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
STASHED=false

for branch in $BRANCHES; do
  # Check exclusion list
  if echo "$EXCLUDE" | tr ',' '\n' | grep -qx "$branch"; then
    echo "SKIP (excluded): $branch"
    SKIPPED=$((SKIPPED + 1))
    SKIPPED_NAMES="$SKIPPED_NAMES  - $branch (excluded)\n"
    continue
  fi

  # Get branch info (commit count + tip SHA)
  BRANCH_INFO=$(but show "$branch" --json | python3 -c "
import sys, json
d = json.load(sys.stdin)
commits = d.get('commits', [])
print(len(commits))
if commits:
    print(commits[0]['sha'])
else:
    print('')
")
  COMMIT_COUNT=$(echo "$BRANCH_INFO" | head -1)
  TIP_SHA=$(echo "$BRANCH_INFO" | tail -1)

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
    if [ "$TARGET_REMOTE" = "gb-local" ]; then
      but merge "$branch"
    else
      # Merge branch tip into local main via git
      if [ "$STASHED" = false ]; then
        git fetch "$TARGET_REMOTE" "$LOCAL_BRANCH"
        git stash --include-untracked -m "ship.sh: auto-stash before merge"
        STASHED=true
      fi
      git checkout "$LOCAL_BRANCH"
      git merge "$TIP_SHA" --no-ff -m "Merge branch '$branch'"
      git checkout gitbutler/workspace
    fi
    MERGED_NAMES="$MERGED_NAMES  - $branch ($COMMIT_COUNT commits)\n"
    sleep 1
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

# Push main to remote after all merges (non-dry-run only)
if [ "$DRY_RUN" = false ] && [ $MERGED -gt 0 ] && [ "$TARGET_REMOTE" != "gb-local" ]; then
  echo ""
  echo "Pushing $LOCAL_BRANCH to $TARGET_REMOTE..."
  git checkout "$LOCAL_BRANCH"
  git pull --rebase "$TARGET_REMOTE" "$LOCAL_BRANCH"
  git push "$TARGET_REMOTE" "$LOCAL_BRANCH"
  git checkout gitbutler/workspace
  if [ "$STASHED" = true ]; then
    git stash pop || true
  fi
  echo "Syncing GitButler..."
  but pull
elif [ "$STASHED" = true ]; then
  git stash pop || true
fi
