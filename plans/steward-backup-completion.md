# Steward Backup Completion — Finish Phase 1 of Community-Anchored Auth

## Who / Where

- **Agent:** agent3 (worktree: `/Users/truman/Code/IndrasNetwork/agent3`, branch: `agent3`)
- **Parent plan:** `~/.claude/plans/i-have-a-kind-lexical-breeze.md` — 3-phase community-anchored auth. This plan closes out **Phase 1** (Shamir steward recovery).
- **Sibling peers:** agent1 is doing `brisk-orbiting-lantern` (braid-sync app wiring, backend-only). agent2 is doing the braid-sync UI. Stay out of their lanes: no Dioxus component work for the braid dashboard, no changes to `ipc.rs` / `sync_panel.rs` / `vault/sync_all.rs` / `vault/view_models.rs` / `vault_manager.rs` GC loop.
- **Check peer state before non-trivial slices:** `~/.claude/scripts/syncgit/syncgit status`.

## Ground rules (repeat for every session)

1. **Plans live in `./plans/` in this worktree, NOT `~/.claude/plans/`.** Per `CLAUDE.md`. `~/.claude/plans/` is sibling peers' territory; reading other agents' plan files caused a real confusion last session.
2. **Never `cargo test` unscoped** — project memory says it hangs. Always `cargo test -p <crate>`.
3. **Run commands from repo root** (`/Users/truman/Code/IndrasNetwork/agent3`), not from subdirectories. Use `cargo build -p <crate>` / `cargo run -p <crate>`.
4. **Use `/sync`**, not manual `git commit` / `git push`. When dirty tree blocks rebase, stash the unrelated files (not this slice's), rebase, push, pop.
5. **Greenfield** — no backward-compat shims. Delete/replace freely.
6. **Design philosophy** (per `CLAUDE.md`): frictionless — inline editing over edit modes, autosave over save buttons, no confirmation dialogs for reversible actions.
7. **Every public type/function gets `///` doc comments.** Every `lib.rs` gets `//!` module docs. New modules → update that crate's `AGENTS.md`.

## State at plan creation (2026-04-21)

### What already landed on `agent3` HEAD

Commits (most recent first):
- `b94ec08 chore: gitignore per-peer scratch files` — this session
- `1a7518b docs: require per-worktree ./plans/ for plan files` — this session
- `161fa3d docs: STATUS.md — fancy-wiggling-pony complete, next plan drafted` — agent1 peer PR
- `37e3cc8 feat: K-of-N steward recovery for the pass-story keystore` — **Phase 1 scaffold** (our earlier slice)
- `a183a61 test: full-cycle ref counting -> rollup -> staged deletion -> blob GC` — agent1's braid work

**Phase 1 scaffold, already in HEAD:**
- `crates/indras-crypto/src/shamir.rs` — K-of-N primitive (10 tests)
- `crates/indras-crypto/src/steward_share.rs` — per-steward ML-KEM-768 envelope (5 tests)
- `crates/indras-sync-engine/src/steward_recovery.rs` — `prepare_recovery` / `recover_encryption_subkey` / `save_manifest` / `load_manifest` + `StewardManifest` JSON at `<data_dir>/steward_recovery.json` (7 tests)
- `crates/indras-sync-engine/src/story_auth.rs::prepare_steward_recovery` — re-derives encryption subkey from pass story, splits, encrypts shares, saves manifest (2 tests)

### What's in the dirty working tree (ready to commit as slice 1)

Modified:
- `crates/indras-sync-engine/src/peer_key_directory.rs` — parallel `kem_keys: BTreeMap<UserId, Vec<u8>>` + `publish_kem` / `get_kem` / `peers_with_kem` (new tests included). **Docstring updated** to describe both key types.
- `crates/indras-sync-engine/src/vault/mod.rs` — `Vault::setup` now takes a `pq_kem_ek: Vec<u8>` param and publishes it to the `peer-keys` doc alongside the verifying key. Three call sites (`create`, `join`, `attach`) thread the KEM bytes in from `network.node().pq_kem_keypair().encapsulation_key_bytes()`.
- `crates/synchronicity-engine/src/recovery_bridge.rs` — `AvailableSteward { label, ek_hex }` + `async fn list_available_stewards(network)` that walks `network.conversation_realms()` and collects peers with published KEM keys. **Display name currently falls back to `Peer abcd1234`** (first 8 hex chars of UserId); real resolution is slice 2.
- `crates/synchronicity-engine/src/components/recovery_setup.rs` — peer-picker section (`FROM YOUR PEERS`) added to the Backup-plan overlay; one-click adds fill an empty steward card.
- `crates/synchronicity-engine/src/components/home_vault.rs` — mounts `RecoverySetupOverlay { state, network }` at the bottom alongside other overlays.
- `crates/synchronicity-engine/assets/styles.css` — `.recovery-picker*` styles.

Untracked:
- `crates/indras-sync-engine/tests/steward_recovery_e2e.rs` (242 lines) — 3 E2E scenarios: lock keystore → collect K shares → reassemble → re-authenticate → load PQ identity → sign & verify.

**Commit message for slice 1:**
```
feat: peer-picker infra — KEM publication + list_available_stewards

Vault setup now publishes the ML-KEM-768 encapsulation key alongside
the DSA verifying key. `PeerKeyDirectory` gains a parallel `kem_keys`
map and `peers_with_kem()` enumerator. The Backup-plan overlay uses
this to show a one-click peer picker; labels still fall back to hex
until display-name resolution lands.

Also adds `tests/steward_recovery_e2e.rs` covering the full loop.
```

## Phasing

Four slices. Each ends in a `/sync` commit. After each slice, update `progress.md` and append a `sessions.md` entry.

### Slice 1 — Commit the enabling infrastructure (~10 min)

The dirty tree is ready. Nothing to write.

- [ ] Verify `cargo build -p indras-sync-engine` clean
- [ ] Verify `cargo build -p synchronicity-engine` clean
- [ ] Run `cargo test -p indras-sync-engine` — existing + new E2E tests should all pass
- [ ] Run `cargo test -p indras-crypto` — regression check
- [ ] `/sync` with the commit message above

### Slice 2 — Display-name resolution for the peer picker (~1–2 hr)

Goal: friendly labels in the peer picker instead of `Peer abcd1234`.

**Why it's tricky:** `UserId = blake3(pq_verifying_key_bytes)` lives in `peer_key_directory`. `MemberId = iroh NodeId` lives in `peer_profile_doc_key(member_id)` for the profile mirror. They're two different identifiers for the same peer; no direct mapping exists on disk.

**Approach (verified in-repo):** For each DM realm, `network.dm_peer_for_realm(&realm_id)` gives the peer MemberId (the only non-self member). Load `_peer_profile:{member_id_hex}` via `crate::profile_bridge::load_peer_profile_from_dm(&net, peer_mid, *realm_id.as_bytes())`. In a DM realm there are exactly two members, so whatever non-self UserId appears in `peers_with_kem()` corresponds to that peer. For **shared/multi-peer realms**, no direct mapping works without extra walk of all `_peer_profile:*` docs + matching on `ProfileIdentityDocument.public_key` — defer unless trivial.

**Reference implementations to mirror:**
- `crates/synchronicity-engine/src/components/home_vault.rs:163-189` already does DM peer → display name for the peer bar. Match that pattern.
- `crates/synchronicity-engine/src/components/peer_profile_popup.rs:55-82` shows the full `load_peer_profile_from_dm` + live liveness enrichment pattern.

**Edit scope:**
- `crates/synchronicity-engine/src/recovery_bridge.rs::list_available_stewards` — for each `realm_id` in `network.conversation_realms()`:
  - If `network.dm_peer_for_realm(&realm_id)` returns a MemberId, `load_peer_profile_from_dm` → `display_name.trim()` → use if non-empty
  - Collect `(UserId, ek)` from `peers_with_kem()`, dedupe across realms, prefer the first non-empty display name seen
  - Fall back to `Peer {uid_hex[..8]}` when no profile resolves
- No UI changes — `recovery_setup.rs` already reads `peer.label`.

**Verify:**
- [ ] Unit / smoke test at whatever level makes sense — if `list_available_stewards` is reachable in a test harness, seed two peers (one with a profile mirror set, one without) and assert labels
- [ ] `cargo build -p synchronicity-engine` clean
- [ ] Manual: `cargo run -p synchronicity-engine`, sign in, click `· Backup plan`, confirm the peer picker shows real display names for DM peers

### Slice 3 — Recovery-side UI (mirror of the Backup-plan overlay) (~3–5 hr)

Goal: close the loop so a user can *use* the shares they've collected.

**Today:** Backup-plan overlay generates and emits shares; no reverse flow exists.

**New overlay:** `crates/synchronicity-engine/src/components/recovery_use.rs` — call it "Use my backup" or similar (follow plain-language copy rules in `DESIGN.md`, no crypto-algorithm names in UI).

Sections (mirror the Backup-plan layout):
1. **Intro** — "Lost your device / forgot your story? Paste the pieces your friends gave you back here."
2. **Share inputs** — variable number of textareas, one per share. `+ Add another` button. Each textarea accepts a hex-encoded `EncryptedStewardShare`.
3. **Steward decapsulation keys** — for the test/dev flow, a toggle "Use a fake friend's secret" that accepts a hex decap key per share so the user can actually decrypt without real steward cooperation. (Steward-held shares will be the in-band flow in Slice 4.)
4. **Keystore token input** — whatever the `recover_encryption_subkey` API needs. Read `steward_recovery.rs` for the exact signature.
5. **Output** — on success: "You're back in." On failure: human-readable error. Slot values get re-populated into `AppState` for immediate use.

**Bridge:** extend `recovery_bridge.rs` with `async fn use_steward_recovery(encrypted_shares_hex: Vec<String>, decap_keys_hex: Vec<String>, ...) -> Result<(), String>`. Spawn-blocking wrap around the sync-engine call, same as `setup_steward_recovery`.

**Trigger:** add a status-bar link next to `· Backup plan`, text `· Use backup`. File: `crates/synchronicity-engine/src/components/status_bar.rs`.

**Mount:** add to `home_vault.rs` alongside `RecoverySetupOverlay`.

**Styles:** reuse `.recovery-*` where possible. New styles under `.recovery-use-*` if needed.

**Verify:**
- [ ] Integration test extending `tests/steward_recovery_e2e.rs` if the UI bridge is testable; otherwise rely on the existing library-level E2E + manual UX
- [ ] Manual: setup backup → grab K shares + their decap keys (via `generate_test_steward_keypair`'s returned dk) → paste into Recovery overlay → assert re-auth succeeds

### Slice 4 — In-band share distribution over iroh (~4–8 hr)

Goal: stewards actually receive their shares without hex copy-paste.

**Wire types** — new module `crates/indras-sync-engine/src/share_delivery.rs`:
```rust
pub struct ShareDelivery {
    pub encrypted_share: Vec<u8>, // serialized EncryptedStewardShare
    pub sender_user_id: UserId,
    pub created_at: i64,
    pub label: String, // human hint, e.g. "Alex's backup piece from Truman"
}

pub struct RecoveryRequest { /* placeholder for slice that asks stewards to release */ }
pub struct ShareRelease { /* placeholder */ }
```

**Delivery mechanism** — publish each share as a doc entry in the *sender's* DM realm with the target steward, keyed by something like `_steward_share:{sender_user_id_hex}`. The steward's side polls their DM realms for new keys matching `_steward_share:*` and ingests them into a local "backups I hold for others" store.

**Steward-side storage** — new file `<data_dir>/steward_holdings.json` (parallel to `steward_recovery.json` but on the other side). Struct: `HashMap<SenderUserId, StoredShareDelivery>`.

**Steward-side UI notification** — minimal: a status-bar badge on `· Backup plan` when you're holding backups for someone, or a "Backups I'm keeping" collapsible list in the overlay. Defer to a later slice if it balloons.

**Recovery-side collection** — when the sender later runs the Recovery overlay (Slice 3), auto-populate share textareas from the DM-realm docs their stewards published back (requires a `release` flow). For the first cut, keep the manual paste path too.

**Verify:**
- [ ] New test `tests/in_band_share_delivery.rs` — two vault peers, one sets up backup nominating the other, assert the other vault's `steward_holdings.json` has the delivery within N iroh gossip ticks.
- [ ] Manual: full loop between two `synchronicity-engine` instances (see `./scripts/run-home-viewer.sh`-style helper — may need a new `scripts/run-two-peers.sh`).

## Critical files (code map)

Already landed in HEAD — read these before editing:
| File | Role |
|---|---|
| `crates/indras-crypto/src/shamir.rs` | K-of-N primitive (sharks wrapper) |
| `crates/indras-crypto/src/steward_share.rs` | `EncryptedStewardShare` + ML-KEM envelope |
| `crates/indras-sync-engine/src/steward_recovery.rs` | `prepare_recovery` / `recover_encryption_subkey` + manifest JSON |
| `crates/indras-sync-engine/src/story_auth.rs` | `prepare_steward_recovery` integration point |
| `crates/indras-network/src/network.rs` | `conversation_realms()`, `dm_peer_for_realm()`, `home_realm()` |
| `crates/indras-sync-engine/src/profile_identity.rs` | `ProfileIdentityDocument` schema (display_name lives here) |
| `crates/synchronicity-engine/src/profile_mirror.rs` | `peer_profile_doc_key(member_id)` = `_peer_profile:{hex}` |
| `crates/synchronicity-engine/src/profile_bridge.rs::load_peer_profile_from_dm` | DM peer's profile read |
| `crates/synchronicity-engine/src/components/home_vault.rs:163-189` | reference pattern for peer → display_name enrichment |

Dirty / in this slice:
| File | Role |
|---|---|
| `crates/indras-sync-engine/src/peer_key_directory.rs` | + `kem_keys` map, `publish_kem`, `get_kem`, `peers_with_kem` |
| `crates/indras-sync-engine/src/vault/mod.rs` | `Vault::setup` also publishes KEM ek |
| `crates/synchronicity-engine/src/recovery_bridge.rs` | + `AvailableSteward` + `list_available_stewards` |
| `crates/synchronicity-engine/src/components/recovery_setup.rs` | peer-picker UI |
| `crates/synchronicity-engine/src/components/home_vault.rs` | mounts recovery overlay |
| `crates/synchronicity-engine/assets/styles.css` | `.recovery-picker*` |
| `crates/indras-sync-engine/tests/steward_recovery_e2e.rs` | full-loop E2E (untracked) |

To create in Slices 3 / 4:
| File | Slice | Role |
|---|---|---|
| `crates/synchronicity-engine/src/components/recovery_use.rs` | 3 | "Use my backup" overlay |
| `crates/synchronicity-engine/src/components/status_bar.rs` | 3 | add `· Use backup` link (edit) |
| `crates/indras-sync-engine/src/share_delivery.rs` | 4 | wire types for in-band delivery |
| `crates/indras-sync-engine/tests/in_band_share_delivery.rs` | 4 | delivery E2E |

## Verification strategy

- `cargo build -p indras-sync-engine` and `cargo build -p synchronicity-engine` must stay clean through every slice.
- `cargo test -p indras-crypto`, `cargo test -p indras-sync-engine`, `cargo test -p synchronicity-engine` scoped only.
- Never `cargo test` unscoped (hangs).
- Manual UX at end of each slice: `cargo run -p synchronicity-engine`, sign in, exercise the Backup / Recovery flow.

## Out of scope for this plan

- **Phase 2** (per-device keys + passkeys + proximity pairing) — huge architectural lift; separate plan when Phase 1 ships.
- **Phase 3** (peer cross-signing + story demotion) — depends on Phase 2.
- **Braid sync app wiring** — agent1 owns `brisk-orbiting-lantern`; do not touch `ipc.rs`, `sync_panel.rs`, `vault/sync_all.rs`, `vault/view_models.rs`, `vault_manager.rs` GC loop.
- **Shared-realm display-name resolution** — Slice 2 is DM-only; multi-peer resolution deferred unless trivial once the DM case lands.

## Open questions to surface to the user

- Slice 4 needs a concrete wire protocol. The plan above sketches one (per-sender doc keyed `_steward_share:*` in the DM realm); confirm that matches the user's mental model before implementing.
- Steward-side "I'm holding a backup for N" notification: inline chip in existing UI, or a dedicated view? Defer to the user once Slice 3 is in their hands.
- Does recovery need a "rehearsal" flow (practice without actually rebuilding the keystore)? Parent plan mentions retiring story rehearsal; per-user practice of steward recovery might be the replacement. Slice 3 lays the groundwork but no explicit rehearsal mode yet.

## Pointers

- Parent plan: `~/.claude/plans/i-have-a-kind-lexical-breeze.md` (do not edit — sibling agents may be reading)
- Design system: `DESIGN.md` (plain language, no crypto-algorithm names in UI)
- Community identity article: `articles/the-heartbeat-of-community.md`
- CLAUDE.md has the frontend design philosophy, syncgit workflow, `cargo test` rules
- This worktree: `/Users/truman/Code/IndrasNetwork/agent3`
