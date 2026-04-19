# agent1 — Current Status (2026-04-19)

## What just shipped

**Plan `fancy-wiggling-pony` is complete** (all 5 phases, 10 sessions). Hierarchical braid sync is fully implemented in the library layer.

Library (`indras-sync-engine`):
- Core CAFS types: `SymlinkIndex`, `ContentAddr`, `IndexDelta`, `LogicalPath`, `Conflict` (`crates/indras-sync-engine/src/content_addr.rs`)
- `AgentBraid` — local-only inner DAG (`src/braid/agent_braid.rs`)
- `Vault::promote()` bridges inner HEAD → outer signed changeset, with auto inner-rollup (`src/vault/mod.rs`)
- `Vault::agent_land` / `merge_agent` / `gc_blobs` routing methods
- `BraidDag::all_referenced_addrs` / `live_addrs` / `rollup` / `descendants_inclusive` (`src/braid/dag.rs`)
- `StagedDeletion` + `StagedDeletionSet` + `DEFAULT_GRACE_PERIOD_MS` (7d) + `DEFAULT_OUTER_RETENTION_MS` (30d) (`src/braid/gc.rs`)
- `WorkspaceHandle::land_to_inner_braid` (`crates/synchronicity-engine/src/team.rs`)

Tests green across the board: 58 braid lib tests, 4 promote integration tests, 1 workspace-wire test, 1 gc_blobs test, 1 full-cycle GC test.

Plan files: `~/.claude/plans/fancy-wiggling-pony.{md,progress.md,sessions.md}`.

## What's NOT wired

The library surface is complete but the `synchronicity-engine` desktop app still uses the pre-braid commit path:
- `ipc.rs` — agent socket commits call `realm.try_land(..)` directly on the outer DAG (bypasses inner braid)
- `components/sync_panel.rs` — same pattern
- No UI for merge/promote/gc
- No background blob GC task

`WorkspaceHandle::land_to_inner_braid` compiles but has zero production callers.

## Next work — proposed plan `brisk-orbiting-lantern` (NOT YET SAVED)

**Important constraints from the user:**
- Follow the dashboard design at `refs/Braid Dashboard — Synchronicity Engine Prototype.html`
- **Agent 2 is doing the UI** — agent1's scope is **backend wiring only**. Do not ship Dioxus components.

**Phase 1** — Rewire `ipc.rs` to call `WorkspaceHandle::land_to_inner_braid` instead of `realm.try_land`. Extend `SyncRequest` with optional `evidence` fields.

**Phase 2** — `Vault::sync_all(intent)` composite: merge agent forks → promote if inner HEAD diverges → auto-merge trusted peers → broadcast. Returns a `SyncAllReport`.

**Phase 3** — View-model accessors for the UI drawers (agent2 will call these):
- `Vault::agent_forks_view(&roster)`
- `Vault::peer_heads_view()`
- `Vault::recent_commits_view(limit)`

**Phase 4** — Blob GC background task in `VaultManager` using the existing `StagedDeletionSet`. 15-minute interval by default.

**Non-goals:** no outer-DAG rollup UI, no persistent staged-deletion across restarts, no changes to indras-sync-engine public API beyond Phases 2–3.

## How to pick up

1. Confirm the plan with the user (it's drafted in-conversation; not yet written to disk). Adjust slug/scope if they redirect.
2. Once approved, save via plan-driver: `~/.claude/plans/brisk-orbiting-lantern.md` + progress + sessions, and update `~/.claude/projects/-Users-truman-Code-IndrasNetwork/memory/project_active_plan.md`.
3. Start with Phase 1 — the IPC rewire is the smallest slice and exercises the inner-braid path end-to-end.

## Coordination with agent2

Agent2 is working the UI against the same dashboard design. Read `~/.claude/plans/fancy-wiggling-pony.sessions.md` to see recent commits from agent1; check `syncgit status` before starting work to pull agent2's PRs. Their UI will need the view-model methods from Phase 3, so get those merged early.

## Repo policy reminders

- `/sync` workflow, not manual `git push` / `git commit`
- Tests scoped to `-p <crate>` — never full `cargo test` (hangs)
- Working directory: `/Users/truman/Code/IndrasNetwork` (do not `cd` into subdirs)
- Follow `project_braid_local_working_tree` — agent disk edits stay local until `/sync`
