# Sessions: Steward Backup Completion

## 2026-04-21 — plan drafted
- Session focus: figure out agent3's lane and draft a resumable plan.
- Discovered `~/.claude/plans/` contains sibling agents' plans (agent1's `brisk-orbiting-lantern`, agent2's UI work) — wrong territory for agent3.
- Updated `CLAUDE.md` to require per-worktree `./plans/` for plan files and gitignored per-peer scratch (`STATUS.md`, `NOTE.md`, `.syncgit/`). Broadcast via `/sync`.
- Drafted this plan. Plan files live in `./plans/` under the worktree.
- Pre-existing dirty auth work (KEM publication + peer picker) is Slice 1; ready to commit with no further editing.
- Slices 2–4 sketched with file-level scope, reference patterns to mirror, and unknowns to surface to the user.
- Next session: `resume` this plan, confirm slice 1 builds clean, and `/sync` it.

## 2026-04-21 — slice 1 landed
- Verified `cargo build -p indras-sync-engine` + `cargo build -p synchronicity-engine` clean.
- `cargo test -p indras-crypto` → 134/134.
- `cargo test -p indras-sync-engine --lib` → 303/303; `--test steward_recovery_e2e` → 3/3.
- Discovered 7 integration tests in braid land (`braid_two_peer`, `braid_three_peer`, `braid_sync_wiring`, `human_sync_and_merge`, `pq_signature_e2e`, `pq_signature_multi_peer`, `fork_rights_e2e`) fail to **compile** — signatures stale after agent1's content-addressed migration (`09d9a28`). Confirmed pre-existing by stashing slice 1 changes (`git stash --include-untracked`) and running `cargo test --no-run` against clean HEAD — same errors. Not this slice's responsibility; flag to agent1.
- Broadcast slice 1 via `/sync`.
- Next: Slice 2 — wire DM peer display-name resolution into `list_available_stewards`.

## 2026-04-21 — slices 2 & 3 landed
- **Slice 2** — `list_available_stewards` now walks each conversation realm, and for DMs (single-non-self member) resolves the peer's display name via `load_peer_profile_from_dm`. Shared-realm-only peers keep the hex-prefix fallback. Build clean; shipped.
- During Slice 2 rebase, absorbed agent-sibling commit `eac82a3 fix: unfreeze indras-sync-engine tests after Changeset/IndexDelta migration` — the braid test-compile breakage flagged after Slice 1 is now resolved upstream.
- **Slice 3** — recovery-side loop:
  - Added `RecoveryContribution` + `use_steward_recovery` to `recovery_bridge.rs`. Spawn-blocking wraps per-share decrypt (`EncryptedStewardShare::decrypt` needs a rebuilt `PQKemKeyPair`, so the overlay collects both dk and ek hex per piece), Shamir recombine, keystore re-auth against the on-disk `story.token`, and a prove-unlock via `load_or_generate_pq_identity`.
  - New component `components/recovery_use.rs` mirrors the Backup-plan layout: per-piece card with label + share + dk + ek textareas, K stepper, status banner, and a Done button after success.
  - `show_recovery_use: bool` added to `AppState`; status-bar gets a `· Use backup` link beside `· Backup plan`; overlay mounted alongside `RecoverySetupOverlay` in `home_vault.rs`.
  - Added `indras-node = { path = "../indras-node" }` to `synchronicity-engine/Cargo.toml` — `StoryKeystore::authenticate` lives in that crate and wasn't reachable through re-exports.
  - No new CSS; reused existing `.recovery-*`.
- Manual two-peer UX verify for both slices still pending — needs a running app.
- Next: Slice 4 — confirm wire protocol with user, then build in-band share delivery.

## 2026-04-21 — slice 4 landed
- Protocol confirmed with user: DM-realm piggyback, `_steward_share:{sender_uid_hex}` key, one share per sender (re-splits overwrite), status-bar badge for stewards, DM-only (home-realm reconnect carries shared-realm rejoin).
- New module `indras-sync-engine/src/share_delivery.rs`:
  - `ShareDelivery` CRDT doc (encrypted_share bytes, sender_user_id, created_at_millis, label) with last-writer-wins merge on timestamp.
  - `share_delivery_doc_key(sender_uid)` → `_steward_share:{hex}`.
  - `StewardHoldings` wrapper (BTreeMap by sender hex) persists to `<data_dir>/steward_holdings.json`.
  - 3 unit tests covering key stability, merge ordering, on-disk roundtrip.
- Sender side (`recovery_bridge::setup_steward_recovery`):
  - Switched input from `Vec<(label, ek)>` to `Vec<StewardInput>` with optional `user_id_hex` — picker-sourced rows carry the peer UID; manual-paste rows stay `None`.
  - Return type → `SetupOutcome { shares_hex, delivered_to }`. In-band delivery is best-effort (failures don't bubble up; hex fallback always surfaces).
  - `dm_realm_map` helper walks `conversation_realms` + peer-keys docs to build UID→RealmId.
- Receiver side (`recovery_bridge::refresh_held_backups`): probes each DM realm's non-self UIDs for the share-delivery doc; non-default hits are materialized into `StewardHoldings` and persisted.
- `AppState.held_backups_count` seeded from disk in `AppState::new` via `recovery_bridge::load_held_backups`; refreshed whenever the Backup-plan overlay opens. Status bar shows `· Backup plan · holding N for friends` when non-zero.
- `AvailableSteward` extended with `user_id_hex`; picker click-handler in `recovery_setup.rs` stores the UID on the `StewardRow` so the setup call can route in-band. "Try with a fake friend" clears `user_id_hex` so the fake stays manual-only.
- Build clean for both crates. `cargo test -p indras-sync-engine --lib` → 306/306; `--test steward_recovery_e2e` → 3/3.
- **Deferred** within Slice 4:
  - `tests/in_band_share_delivery.rs` two-peer iroh E2E — covered pattern exists in `pq_signature_multi_peer.rs`; follow-up item.
  - Recovery-side auto-populate from DM-realm docs — design-gated: a broken-device user can't join DM realms without identity, so stewards still need to send shares out-of-band on recovery. In-band helps the *setup* side today; Phase 2 cross-device identity unlocks recovery-side auto-pull.
