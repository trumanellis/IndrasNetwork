# Brisk Orbiting Lantern — Braid Sync App Wiring

## Context

The hierarchical braid sync library shipped with `fancy-wiggling-pony` (completed 2026-04-19, all 5 phases). The library surface is complete — `AgentBraid`, `Vault::promote`, inner/outer DAG split, `StagedDeletion`, and `WorkspaceHandle::land_to_inner_braid` all exist and are tested (58 braid lib tests + 4 promote integration + 1 workspace-wire + full-cycle GC).

**But the `synchronicity-engine` desktop app is still using the pre-braid commit path.** Specifically:

- `crates/synchronicity-engine/src/ipc.rs` — the Claude Code agent socket commits call `realm.try_land(..)` directly on the outer DAG, bypassing the inner braid entirely.
- `crates/synchronicity-engine/src/components/sync_panel.rs` — same pattern: the UI's Commit button calls `realm.try_land`.
- `WorkspaceHandle::land_to_inner_braid` compiles but has zero production callers.
- No UI for merge/promote/GC, and no background blob GC task.

This plan is scoped to the **backend wiring only** — agent2 is working the UI side concurrently against the dashboard at `refs/Braid Dashboard — Synchronicity Engine Prototype.html`, so we intentionally ship no Dioxus components here.

## Phasing

### Phase 1 — Rewire app entry points to the inner braid

Swap the two entry points from `realm.try_land` to `WorkspaceHandle::land_to_inner_braid`.

Files to modify:
- `crates/synchronicity-engine/src/ipc.rs`
  - Extend `SyncRequest` with `evidence: Option<EvidencePayload>` — a lightweight wire-format struct for `Evidence::Agent { compiled, tests_passed, lints_clean, runtime_ms }`. `signed_by` is filled in from the network's `PQIdentity` on the server, not the client.
  - Replace the `realm.try_land(..)` call with a `WorkspaceHandle::land_to_inner_braid(&vault, intent, evidence)` call. Server looks up the `WorkspaceHandle` matching the request `cwd`, then holds the `VaultManager` read lock to get the `&Vault` for that workspace's realm.
  - Take `Arc<Vec<WorkspaceHandle>>` in place of the raw `Vec<Arc<LocalWorkspaceIndex>>` so the handler has the handle (not just the index).
  - Default evidence when absent: `Evidence::Agent { compiled: false, tests_passed: vec![], lints_clean: false, runtime_ms: 0, signed_by: derive_agent_id(&user_id, agent) }`.
- `crates/synchronicity-engine/src/components/sync_panel.rs`
  - Replace the `realm.try_land(..)` call in `commit_for_agent` with `handle.land_to_inner_braid(&vault, intent, evidence)`. Evidence is the same default for now — later phases can thread real verification results.
  - Drop the `publish_and_materialize_head` call here; with the inner-braid path, materialization and peer-head publish happen on `Vault::promote` (added in Phase 2's composite). Until the composite lands, the user must invoke promote to see files appear — acceptable trade for the UI refactor agent2 is shipping.
- `crates/synchronicity-engine/src/main.rs` (or wherever `start_ipc_server` is called) — pass workspace handles through instead of indexes.

Callers of `start_ipc_server` change signature; update `components/sync_panel.rs`'s `SyncPanel` props if it held the same indexes list (it passes `workspace_handles` already — no change there).

Verification:
- `cargo build -p synchronicity-engine` clean.
- Existing tests `workspace_to_inner_braid`, `head_persist_and_materialize`, `sync_panel_commit`, `braid_sync_wiring`, `auto_rebase_on_head` all still pass.
- New test: `tests/ipc_lands_into_inner_braid.rs` — bring up two workspaces, POST a `SyncRequest` to the socket, assert the changeset shows up on the inner DAG, not the outer.

### Phase 2 — `Vault::sync_all(intent)` composite

Library-side addition in `crates/indras-sync-engine/src/vault/mod.rs`.

New public method:
```rust
pub async fn sync_all(&self, intent: String) -> Result<SyncAllReport, VaultError>
```

Steps, in order:
1. `merge_all_agents()` — collapse every agent fork into the user's inner HEAD.
2. If inner HEAD diverges from outer HEAD, call `self.promote(intent)` — this builds the signed outer changeset, updates `peer_heads`, and does the aggressive inner-rollup that `fancy-wiggling-pony` Phase 5 introduced.
3. `auto_merge_trusted()` on the outer DAG — pull in any trusted peer forks that arrived since last sync.
4. Materialize files to the vault root (existing `publish_and_materialize_head` logic moved into the Vault, or reused).
5. Return a `SyncAllReport` with: `agent_merges: Vec<(LogicalAgentId, ChangeId)>`, `promoted: Option<ChangeId>`, `peer_merges: Vec<(UserId, ChangeId)>`, `materialized: usize`, `conflicts: Vec<Conflict>`.

Files to create / modify:
- `crates/indras-sync-engine/src/vault/sync_all.rs` — new module with `SyncAllReport` + `sync_all` impl.
- `crates/indras-sync-engine/src/vault/mod.rs` — wire the new module, re-export `SyncAllReport`.
- `crates/indras-sync-engine/tests/vault_sync_all.rs` — integration test: 2 agents land concurrently, call `sync_all`, assert agent forks merged, single outer changeset promoted, peer fork auto-merged.

Rewire IPC and `sync_panel`'s commit path to call `Vault::sync_all` after `land_to_inner_braid`, so a single `/sync`-equivalent from the UI or agent socket lands → merges → promotes → broadcasts.

### Phase 3 — View-model accessors for agent2's UI

Agent2 is building drawers against the braid dashboard design. They need read-only, cheap-to-compute view models:

- `Vault::agent_forks_view(&roster) -> Vec<AgentForkView>` — one entry per bound agent whose inner HEAD differs from the user's inner HEAD. Fields: `agent: LogicalAgentId`, `head: ChangeId`, `delta_count: usize` (files changed vs. user HEAD), `last_intent: String`, `last_ts: u64`.
- `Vault::peer_heads_view() -> Vec<PeerHeadView>` — one entry per peer in `peer_heads`, showing `peer: UserId`, `head: ChangeId`, `ahead_of_self: bool`, `trusted: bool`.
- `Vault::recent_commits_view(limit: usize) -> Vec<CommitView>` — last N outer DAG entries in reverse chronological order. Fields: `id: ChangeId`, `parents: Vec<ChangeId>`, `author: UserId`, `intent: String`, `ts: u64`, `files_touched: usize`.

Files to create:
- `crates/indras-sync-engine/src/vault/view_models.rs` — new module with the three view structs + accessor impls on `Vault`.

Files to modify:
- `crates/indras-sync-engine/src/vault/mod.rs` — wire module, re-export view types.
- `crates/synchronicity-engine/src/vault_manager.rs` — thin passthrough helpers (`agent_forks_view_for(realm_id, roster)`, etc.) so Dioxus components don't need to hold the read lock themselves.

Verification: unit tests on each accessor against a seeded braid state (2 agents, 3 peers, 10 commits).

### Phase 4 — Background blob GC task

Blob GC is already implemented (`Vault::gc_blobs` + `StagedDeletionSet` + `DEFAULT_GRACE_PERIOD_MS` = 7d). What's missing is an app-level scheduler.

Files to modify:
- `crates/synchronicity-engine/src/vault_manager.rs` — add a `start_gc_loop(interval: Duration)` method that spawns a tokio task iterating every vault in the manager every `interval`, calling `vault.gc_blobs()`. Default interval 15 min. The task stores its `JoinHandle` on the manager so shutdown can abort it.
- `crates/synchronicity-engine/src/main.rs` — call `vault_manager.start_gc_loop(Duration::from_secs(15 * 60))` once after vault_manager init.

No persistent staged-deletion across restarts: the `StagedDeletionSet` is in-memory; on restart, GC re-stages whatever is still unreachable. Acceptable for v1; user may revisit if restart frequency is high.

Verification:
- Unit test on `start_gc_loop` using a 50 ms interval + `tokio::time::pause` / `advance` to exercise the loop without real sleeps.
- No new integration test — the library-level `tests/gc_full_cycle.rs` already exercises the collection logic.

## Critical files

Existing:
- `crates/synchronicity-engine/src/ipc.rs` — Phase 1 rewire
- `crates/synchronicity-engine/src/components/sync_panel.rs` — Phase 1 rewire
- `crates/synchronicity-engine/src/vault_manager.rs` — Phase 3 passthroughs + Phase 4 GC loop
- `crates/synchronicity-engine/src/main.rs` — Phase 1 wiring update + Phase 4 loop start
- `crates/synchronicity-engine/src/team.rs` — already exports `WorkspaceHandle::land_to_inner_braid`; no changes
- `crates/indras-sync-engine/src/vault/mod.rs` — module wiring for Phases 2 & 3
- `crates/indras-sync-engine/src/braid/changeset.rs` — `Evidence` definition; unchanged

New:
- `crates/indras-sync-engine/src/vault/sync_all.rs` (Phase 2)
- `crates/indras-sync-engine/src/vault/view_models.rs` (Phase 3)
- `crates/synchronicity-engine/tests/ipc_lands_into_inner_braid.rs` (Phase 1)
- `crates/indras-sync-engine/tests/vault_sync_all.rs` (Phase 2)

## Verification strategy

**Scoped cargo per project memory** — `cargo test -p synchronicity-engine`, `cargo test -p indras-sync-engine`. Never unscoped `cargo test`.

**Phase 1 regression guard:** all existing tests in both crates must still pass. Plus the new `ipc_lands_into_inner_braid` test asserts the rewire actually happened.

**Phase 2 integration test:** full-cycle on a 2-agent, 2-peer vault: both agents land; `sync_all` merges both, promotes one outer changeset, pulls in the peer fork, materializes files. Assertions on every field of `SyncAllReport`.

**Phase 3 unit tests:** each view accessor against a hand-built braid state.

**Phase 4:** tokio time-mocked loop test.

**Manual UX validation** (end of Phase 2, before Phase 3): `./scripts/run-home-viewer.sh` — start the syncengine, POST a commit through the agent socket, confirm inner HEAD advances, then trigger promote (via sync_all) and confirm peer sees the change.

## Out of scope

- Outer-DAG rollup UI (visual history compaction) — separate concern, no library work needed for the dashboard's MVP.
- Persistent `StagedDeletionSet` across restarts — in-memory is acceptable for v1.
- Any changes to `indras-sync-engine` public API outside of `Vault::sync_all` (Phase 2) and the view-model accessors (Phase 3).
- Dioxus component work — agent2's scope.
- Changes to the dashboard HTML prototype — reference-only; agent2 consumes it.

## Coordination with agent2

Agent2 is implementing the UI against the dashboard design. They will consume the Phase 3 view-model methods, so **Phase 3 should merge before the UI phase that depends on it**. Use `syncgit status` before starting any non-trivial slice to check for their PRs. Phase 1 and Phase 2 have no UI cross-dependencies.

## Open questions to revisit after Phase 2

- Should `sync_all` return before or after the broadcast completes (peer-visible)? Returning before is simpler; the UI can poll peer_heads_view for propagation status.
- Does `Evidence` need a third variant for "human-with-agent-assist" commits where both humans and agents contributed? For now `Evidence::Human` and `Evidence::Agent` cover the cases; revisit if mixed commits become common.
