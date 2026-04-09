#!/bin/bash
# PostToolUse hook for Bash: detects successful full test suite runs
# and nudges Claude to land the change to main via the Trunk-Pulse protocol.
#
# Claude Code passes JSON on stdin with:
#   .tool_input.command    - the command that was run
#   .tool_response.exit_code - exit code (0 = success)
#
# Only triggers on full workspace test runs (cargo test without -p flag,
# or cargo test --workspace), not single-crate tests.

set -euo pipefail

# Read JSON from stdin
INPUT_JSON=$(cat)

COMMAND=$(echo "$INPUT_JSON" | jq -r '.tool_input.command // ""')
EXIT_CODE=$(echo "$INPUT_JSON" | jq -r '.tool_response.exit_code // 1')

# Only act on successful commands
[ "$EXIT_CODE" = "0" ] || exit 0

# Match full test suite runs only:
#   cargo test --workspace
#   cargo test  (no -p flag = full workspace)
# Skip single-crate tests (cargo test -p <crate>)
is_full_test() {
  local cmd="$1"

  # Must contain "cargo test"
  echo "$cmd" | grep -q 'cargo test' || return 1

  # If it has --workspace, it's full
  echo "$cmd" | grep -q '\-\-workspace' && return 0

  # If it has -p or --package, it's single-crate — skip
  echo "$cmd" | grep -qE '\s-p\s|--package' && return 1

  # Plain "cargo test" with no package filter = full workspace
  return 0
}

if is_full_test "$COMMAND"; then
  # Check if there are actually changes to land
  DIFF=$(jj diff --stat 2>/dev/null || true)
  if [ -z "$DIFF" ]; then
    exit 0
  fi

  # Output JSON nudge for Claude
  jq -n '{
    hookSpecificOutput: {
      hookEventName: "PostToolUse",
      additionalContext: "[TRUNK-PULSE: TESTS PASSED — LAND THIS CHANGE]\n\nFull test suite passed. Execute the landing protocol:\n\n1. Ensure the change has a good description: `jj describe -m \"feat: <what this change does>\"`\n2. Rebase onto latest main: `jj rebase -d main`\n3. Advance main bookmark: `jj bookmark set main -r @`\n4. Push: `jj git push`\n   - If push fails (another agent landed first): `jj git fetch && jj rebase -d main` then retry from step 3\n5. Start fresh: `jj new main -m \"continue: <next task>\"`\n\nThis pulls in all work landed by other agents automatically."
    }
  }'
fi
