# Progress: Brisk Orbiting Lantern

## Completed
- [x] Phase 1 — Rewire to inner braid (see section below)

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

### Phase 2 — `Vault::sync_all` composite
- [ ] Create `crates/indras-sync-engine/src/vault/sync_all.rs` with `SyncAllReport` + impl
- [ ] Re-export from `vault/mod.rs`
- [ ] Rewire IPC + sync_panel to call `sync_all` after `land_to_inner_braid`
- [ ] Integration test: `crates/indras-sync-engine/tests/vault_sync_all.rs`
- [ ] Run `cargo test -p indras-sync-engine` + `cargo test -p synchronicity-engine` — all pass

### Phase 3 — View-model accessors
- [ ] Create `crates/indras-sync-engine/src/vault/view_models.rs`
- [ ] Implement `Vault::agent_forks_view(&roster)`
- [ ] Implement `Vault::peer_heads_view()`
- [ ] Implement `Vault::recent_commits_view(limit)`
- [ ] Passthrough helpers in `synchronicity-engine/vault_manager.rs`
- [ ] Unit tests for each accessor against seeded braid
- [ ] Notify agent2 in session log so they can wire drawers

### Phase 4 — Background blob GC
- [ ] `VaultManager::start_gc_loop(interval)` with tokio::spawn
- [ ] Call from `main.rs` at 15-min interval
- [ ] Shutdown path: store + abort `JoinHandle`
- [ ] Tokio time-mocked unit test

## Notes
- Backend wiring only — no Dioxus components (agent2's scope).
- Use `cargo test -p <crate>`, never unscoped (project memory).
- Work under the workspace path `/Users/truman/Code/IndrasNetwork/agent1/`, not the parent repo.
- Broadcast via `/sync` skill when each phase lands, not manual `git commit` / `git push`.
