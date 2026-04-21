# Progress: Brisk Orbiting Lantern

## Completed
- [x] Phase 1 — Rewire to inner braid (see section below)
- [x] Phase 2 — `Vault::sync_all` composite (see section below)
- [x] Phase 3 — View-model accessors (covered by agent2 — see section below)
- [x] Phase 4 — Background blob GC task (see section below)

**Plan complete.** All 4 phases landed. Next plan TBD.

## Pending

### Phase 1 — Rewire to inner braid ✅
- [x] Add `Vault::land_agent_snapshot(agent, index, intent, evidence) -> ChangeId` library primitive
- [x] Simplify `WorkspaceHandle::land_to_inner_braid` as a thin delegate
- [x] Add `VaultManager::land_agent_snapshot_on_first` helper
- [x] Extend `SyncRequest` in `ipc.rs` with optional `evidence: Option<EvidencePayload>` wire-format
- [x] Introduce `IpcBinding { agent, index }` and change `start_ipc_server` signature to take `Vec<IpcBinding>`
- [x] Replace `realm.try_land` in `ipc.rs` with `VaultManager::land_agent_snapshot_on_first`
- [x] Replace `realm.try_land` in `components/sync_panel.rs::commit_for_agent`
- [x] Drop `publish_and_materialize_head` from the commit-row path (moves to promote in Phase 2)
- [x] Update `components/app.rs` wiring to build `IpcBinding`s from handles
- [x] Add `VaultManager::inner_braid_contains` / `outer_dag_contains` accessors
- [x] New test: `tests/ipc_lands_into_inner_braid.rs` asserts inner DAG contains commit AND outer DAG doesn't
- [x] Update existing `tests/ipc_sync.rs` for new signature
- [x] `cargo test -p synchronicity-engine` — all pass (24 tests across 6 suites)
- [x] `cargo test -p indras-sync-engine --test sync_panel_commit` — passes

### Phase 2 — `Vault::sync_all` composite ✅
- [x] Create `crates/indras-sync-engine/src/vault/sync_all.rs` with `SyncAllReport` + impl
- [x] Re-export `SyncAllReport` from `vault/mod.rs`
- [x] Integration test: `crates/indras-sync-engine/tests/vault_sync_all.rs` — 2 agents land, sync_all merges/promotes/materializes, second sync_all is a no-op
- [x] `VaultManager::sync_all_on_first(intent, roster)` passthrough
- [x] Extend IPC `SyncResponse` with `promoted` and `peer_merges` fields
- [x] IPC handler calls `sync_all_on_first` after `land_agent_snapshot_on_first`
- [x] `sync_panel::commit_for_agent` builds roster from `workspace_handles`, calls `sync_all_on_first` after the inner-braid land
- [x] Updated `ipc_lands_into_inner_braid` test — renamed `ipc_commit_lands_inner_promotes_and_materializes`, now asserts both inner & outer ids appear, files materialize to vault root
- [x] `cargo test -p indras-sync-engine` — 345 tests pass
- [x] `cargo test -p synchronicity-engine` — 24 tests pass

**Design notes:**
- `sync_all(intent, roster)` takes the roster as a param — matches `agent_forks` / `merge_all_agents` conventions.
- `needs_promote` checks `head_index` equality (not `change_id`), so a re-sync with no new work is a no-op promote.
- Materialization is a private `materialize_user_outer_head` method on `Vault`; blob-load failures are logged, not propagated — the sync still counts as landed.
- IPC path: land → sync_all; `sync_all` error is logged but does not fail the IPC response (inner-braid land already succeeded). Caller sees both `change_id` (inner) and `promoted` (outer).
- Phase-1 `ipc_lands_into_inner_braid` assertion had to be updated: after Phase 2, `Vault::promote`'s aggressive inner-rollup means the original agent change id is GC'd from the inner DAG once it's folded into the user HEAD. The test now asserts inner-vs-outer id distinction instead of persistence.

### Phase 3 — View-model accessors ✅ (deferred-and-covered)
Rather than ship a second set of view models at the library layer, we accept agent2's app-layer implementation as the Phase-3 deliverable:

- `synchronicity-engine/src/state.rs` defines `AgentForkView`, `PeerHeadView`, `CommitView`, `BraidView`, `ConflictView`, `EvidenceView` — display-ready with colors/short-ids/relative times baked in.
- `synchronicity-engine/src/braid_bridge.rs::build_braid_view` translates a raw `BraidDag` into a `BraidView` (peers + commits + pending_forks + conflicts), with lane/slot graph layout. Has unit tests.
- `VaultManager::collect_agent_forks(roster) -> Vec<AgentForkView>` covers the "agent forks" accessor.
- `VaultManager::load_braid_view(realm_id, peers, self_display_name) -> Option<BraidView>` covers peer_heads_view + recent_commits_view.

**Design note:** Plan had called for library-layer raw-data accessors. Agent2 built display-ready view models at the app layer instead — colors/short-ids/relative-time are display concerns, so this layering is actually cleaner. Skipping the library-layer duplication per `feedback_simplicity_over_idiom` (don't build parallel abstractions without concrete justification). If the library later gains a non-UI consumer, revisit.

### Phase 4 — Background blob GC ✅
- [x] `VaultManager::start_gc_loop(interval)` takes `&Arc<Self>`, spawns a tokio task owning a cloned `Arc<VaultManager>`
- [x] `VaultManager::stop_gc_loop()` aborts the task; `Drop` also aborts
- [x] `DEFAULT_GC_INTERVAL = 15 minutes` constant
- [x] Wired in `vault_bridge.rs` (both `create_account` / `restore_account` paths) and `components/app.rs` (bind-first-run path)
- [x] Integration test `vault_manager_gc_loop.rs` — 50ms interval, seeds an unreferenced blob, polls for deletion, asserts stop_gc_loop returns cleanly

**Design notes:**
- Stored the `JoinHandle` behind `std::sync::Mutex<Option<JoinHandle>>` (not tokio's), since it's only accessed on startup/shutdown — std Mutex is cheaper and keeps `start_gc_loop` sync.
- `run_gc_once` is `pub(crate)` so tests can exercise a single pass if they want; the interval task calls the same method.
- First tick is skipped so startup doesn't race vault attach.
- Calling `start_gc_loop` multiple times aborts the previous task and takes over — safe under the multiple-init paths the app has.
- Skipped the "tokio::time::pause + advance" simulation variant: real blob-store IO doesn't play well with paused virtual time. The 50ms polling test is fast enough (~3s ceiling).

## Notes
- Backend wiring only — no Dioxus components (agent2's scope).
- Use `cargo test -p <crate>`, never unscoped (project memory).
- Work under the workspace path `/Users/truman/Code/IndrasNetwork/agent1/`, not the parent repo.
- Broadcast via `/sync` skill when each phase lands, not manual `git commit` / `git push`.
