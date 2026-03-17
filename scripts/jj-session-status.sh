#!/bin/bash
# Outputs jj workspace and change status for Claude Code session start
cd "$(dirname "$0")/.." || exit 0

STATUS=$(jj st 2>/dev/null) || exit 0
WORKSPACES=$(jj workspace list 2>/dev/null) || exit 0
LOG=$(jj log --limit 5 2>/dev/null) || exit 0

cat <<EOF
[JJ WORKSPACE DASHBOARD]
Workspaces:
$WORKSPACES

Current status:
$STATUS

Recent history:
$LOG

Show this dashboard to the user and offer: "Continue an existing change, start new work, or create a parallel workspace?"
EOF
