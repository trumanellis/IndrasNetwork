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

### Slice 2 — Display-name resolution ✓ 2026-04-21
- [x] Edit `recovery_bridge.rs::list_available_stewards` to walk conversation_realms, call `dm_peer_for_realm` + `load_peer_profile_from_dm`, dedupe labels
- [x] `cargo build -p synchronicity-engine` clean
- [ ] Manual verify in running app (deferred — needs two-peer live session)
- [x] `/sync` with commit: "feat: resolve DM peer display names in backup picker"

### Slice 3 — Recovery-side UI (Use-my-backup overlay) ✓ 2026-04-21
- [x] Draft `components/recovery_use.rs` — parallel to `recovery_setup.rs`
- [x] Extend `recovery_bridge.rs` with `use_steward_recovery` + `RecoveryContribution`
- [x] Add `· Use backup` link in `components/status_bar.rs`
- [x] Mount in `components/home_vault.rs` (also add `show_recovery_use` to `AppState`)
- [x] Reuse existing `.recovery-*` styles — no new CSS needed
- [x] Add `indras-node` dep to `synchronicity-engine/Cargo.toml` (needed for `StoryKeystore::authenticate`)
- [ ] Manual verify full setup → collect → paste → re-auth loop (deferred — needs running app)
- [x] `/sync` with commit: "feat: recovery-side overlay — use backup shares to re-authenticate"

### Slice 4 — In-band share delivery over iroh ✓ 2026-04-21
- [x] Confirm wire-protocol shape with user: DM-realm piggyback, one share per sender, status-bar badge, DM-only (home-realm reconnect covers shared realms)
- [x] New module `indras-sync-engine/src/share_delivery.rs` — `ShareDelivery` doc schema (last-writer-wins on `created_at_millis`), `share_delivery_doc_key(sender_uid)`, `StewardHoldings` persistence to `steward_holdings.json` (3 unit tests)
- [x] Publish shares as `_steward_share:{sender_uid_hex}` entries in DM realms — extended `setup_steward_recovery` with optional `IndrasNetwork` + `StewardInput.user_id_hex`; returns `SetupOutcome { shares_hex, delivered_to }` so hex fallback coexists with in-band
- [x] Steward-side scan (`recovery_bridge::refresh_held_backups`) walks DM realms, probes per-peer doc keys, materializes `steward_holdings.json`
- [x] Status-bar badge "holding N for friends" on `· Backup plan` when `held_backups_count > 0`; seeded from disk on `AppState::new`, refreshed when overlay opens
- [x] `cargo build -p synchronicity-engine` + `-p indras-sync-engine` clean
- [x] `cargo test -p indras-sync-engine --lib` → 306/306 (3 new share_delivery)
- [x] `cargo test -p indras-sync-engine --test steward_recovery_e2e` → 3/3
- [ ] **Deferred** — `tests/in_band_share_delivery.rs` two-peer iroh E2E. Unit-level merge & persistence covered; full two-peer gossip test matches the pattern in `tests/pq_signature_multi_peer.rs` and can be added as a follow-up. User can prioritize if needed.
- [ ] Recovery-side auto-populate from DM-realm docs. **Deferred** per design note — when the user runs recovery on a broken device with no network identity, they can't join the DM realm to pull shares; stewards still need to export their shares out-of-band. In-band delivery helps only the *setup* side today. Phase-2 cross-device identity will unlock auto-pull.
- [ ] Two-peer manual verify — needs a running app
- [x] `/sync` with commit: "feat: in-band steward share delivery over iroh DM realms"

## Notes
- Agent3 worktree, branch `agent3`. Sibling peers: agent1 (brisk-orbiting-lantern — braid backend), agent2 (braid UI). Do not touch their files.
- Use `/sync`, not manual git. Stash unrelated dirty state before rebase.
- Scoped `cargo test -p <crate>` only.
- Frontend: frictionless, inline editing, no confirmation dialogs for reversible actions.
- Plain-language UI copy; no crypto-algorithm names in user-visible strings.
