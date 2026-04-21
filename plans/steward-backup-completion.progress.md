# Progress: Steward Backup Completion

## Completed

### Slice 1 — Commit enabling infrastructure ✓ 2026-04-21
- [x] `cargo build -p indras-sync-engine` clean
- [x] `cargo build -p synchronicity-engine` clean
- [x] `cargo test -p indras-sync-engine --lib` — 303 pass
- [x] `cargo test -p indras-sync-engine --test steward_recovery_e2e` — 3/3 pass
- [x] `cargo test -p indras-crypto` — 134 pass
- [x] `/sync` with commit: "feat: peer-picker infra — KEM publication + list_available_stewards"
- Pre-existing agent1 braid test-compile breakage (7 integration tests: `braid_two_peer`, `braid_three_peer`, `braid_sync_wiring`, `human_sync_and_merge`, `pq_signature_e2e`, `pq_signature_multi_peer`, `fork_rights_e2e`) confirmed present on clean HEAD — not caused by this slice. Flag to agent1.

## Pending

### Slice 2 — Display-name resolution
- [ ] Edit `recovery_bridge.rs::list_available_stewards` to walk conversation_realms, call `dm_peer_for_realm` + `load_peer_profile_from_dm`, dedupe labels
- [ ] Smoke/unit test if reachable
- [ ] `cargo build -p synchronicity-engine` clean
- [ ] Manual verify in running app
- [ ] `/sync` with commit: "feat: resolve DM peer display names in backup picker"

### Slice 3 — Recovery-side UI (Use-my-backup overlay)
- [ ] Draft `components/recovery_use.rs` — parallel to `recovery_setup.rs`
- [ ] Extend `recovery_bridge.rs` with `use_steward_recovery` async fn
- [ ] Add `· Use backup` link in `components/status_bar.rs`
- [ ] Mount in `components/home_vault.rs`
- [ ] `.recovery-use-*` styles in `assets/styles.css`
- [ ] Manual verify full setup → collect → paste → re-auth loop
- [ ] `/sync` with commit: "feat: recovery-side overlay — use backup shares to re-authenticate"

### Slice 4 — In-band share delivery over iroh
- [ ] Confirm wire-protocol shape with user before building
- [ ] New module `indras-sync-engine/src/share_delivery.rs`
- [ ] Publish shares as `_steward_share:*` entries in DM realms
- [ ] Steward-side polling + `steward_holdings.json` persistence
- [ ] Steward-side notification UI (minimal)
- [ ] Recovery-side auto-populate from DM-realm docs
- [ ] New E2E test `tests/in_band_share_delivery.rs`
- [ ] Two-peer manual verify
- [ ] `/sync` with commit: "feat: in-band steward share delivery over iroh"

## Notes
- Agent3 worktree, branch `agent3`. Sibling peers: agent1 (brisk-orbiting-lantern — braid backend), agent2 (braid UI). Do not touch their files.
- Use `/sync`, not manual git. Stash unrelated dirty state before rebase.
- Scoped `cargo test -p <crate>` only.
- Frontend: frictionless, inline editing, no confirmation dialogs for reversible actions.
- Plain-language UI copy; no crypto-algorithm names in user-visible strings.
