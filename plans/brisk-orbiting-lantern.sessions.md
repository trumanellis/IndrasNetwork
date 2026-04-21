# Sessions: Brisk Orbiting Lantern

## 2026-04-21 — plan drafted, Phase 1 complete
- Memory refreshed: `fancy-wiggling-pony` removed from paused, `brisk-orbiting-lantern` set active, auth plan Phase 1 marked done and queued.
- Plan files written to `~/.claude/plans/brisk-orbiting-lantern.{md,progress.md,sessions.md}`.
- Approached as backend wiring only per user directive (agent2 owns UI).
- **Phase 1 landed** — IPC + sync_panel both route through `VaultManager::land_agent_snapshot_on_first` onto the vault's inner braid; `Vault::land_agent_snapshot` added as the library primitive; `WorkspaceHandle::land_to_inner_braid` is now a thin delegate. `Evidence` wire format added to `SyncRequest`. New test `ipc_lands_into_inner_braid` asserts inner-braid routing with no outer-DAG leak.
- Discovered pre-existing indras-sync-engine test breakages (`fork_rights_e2e`, `braid_sync_wiring`, `braid_two_peer`) — unrelated to Phase 1, tracked but not fixed here.
- Follow-up sync (same day): fixed all 7 stale test files (`braid_two_peer`, `braid_three_peer`, `braid_sync_wiring`, `fork_rights_e2e`, `human_sync_and_merge`, `pq_signature_e2e`, `pq_signature_multi_peer`) and relocated plan files from `~/.claude/plans/` into `./plans/` per updated CLAUDE.md.
- **Phase 2 landed** — `Vault::sync_all(intent, roster)` composite ships as `crates/indras-sync-engine/src/vault/sync_all.rs` with `SyncAllReport`. `VaultManager::sync_all_on_first` passthrough. Both IPC and `sync_panel` now call `land_agent_snapshot_on_first` then `sync_all_on_first` in one action — every commit is a full braid-sync (merge agents → promote → auto-merge trusted peers → materialize). `SyncResponse` gained `promoted` and `peer_merges` fields. Integration test `vault_sync_all` covers merge+promote+materialize and idempotent resync; Phase-1 test updated to match the new post-rollup semantics.
- Next: Phase 3 view-model accessors (`Vault::agent_forks_view`, `Vault::peer_heads_view`, `Vault::recent_commits_view`).
