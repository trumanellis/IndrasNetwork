# Frictionless Recovery — Social-Threshold Account Architecture

This plan supersedes `steward-backup-completion.md` (Phase 1). It rebuilds recovery around three cohesive ideas:

1. **Logical accounts with pluggable devices.** Identity is an `AccountRoot` that signs `DeviceCertificate`s. Devices come and go; the account persists.
2. **Frictionless steward protocol.** Invitation → acceptance → (later) request → release, all over DM realms. No hex copy-paste, no crypto vocabulary in the UI, peers authenticate by humans verifying humans.
3. **Peer-replicated personal data.** Files erasure-coded (Reed-Solomon) across Backup Peers so device loss is a hydration delay, not data loss.

## Who / Where

- **Agent:** agent3 (worktree: `/Users/truman/Code/IndrasNetwork/agent3`, branch: `agent3`)
- **Parent plan:** `~/.claude/plans/i-have-a-kind-lexical-breeze.md` — the overall community-anchored auth roadmap. Plan A here is the "invitation UX" that was loosely Phase 1 follow-on; Plan B is what `i-have-a-kind-lexical-breeze` originally sketched as "Phase 2 peer cross-signing" but reshaped around a single account root; Plan C is the new backup story implied by the user's vision.
- **Predecessor plan (completed):** `./plans/steward-backup-completion.md`. Phase-1 steward split landed in commits `a84131e…6c16907`. This plan rewrites the UX parts end-to-end — the hex-paste flows, the `RecoveryUseOverlay`, and the setup-time auto-publish all go away.
- **Sibling peers:** agent1 / agent2 have their own in-flight work (braid-sync app wiring, braid UI). Stay out of `ipc.rs`, `sync_panel.rs`, `vault/sync_all.rs`, `vault/view_models.rs`, `vault_manager.rs`'s GC loop, and the braid-dashboard Dioxus components.

## Ground rules (repeat every session)

1. **Plans live in `./plans/` in this worktree, NOT `~/.claude/plans/`.** Per `CLAUDE.md`. Sibling-peer territory is hands-off.
2. **Never `cargo test` unscoped** — always `cargo test -p <crate>`.
3. **Run commands from repo root** (`/Users/truman/Code/IndrasNetwork/agent3`). `cargo build -p <crate>` / `cargo run -p <crate>`.
4. **Use `/sync`**, never manual `git commit` / `git push`. Stash unrelated dirty files before rebase if needed.
5. **Greenfield** — no backward-compat shims. Delete/replace freely.
6. **Design philosophy** (`CLAUDE.md`): frictionless — inline editing, autosave, no confirmation dialogs for reversible actions, direct manipulation over menus.
7. **No crypto-algorithm names in UI copy.** Plain language only. No "ML-KEM", no "Shamir", no "PQ identity", no "encapsulation key". Users see "friend", "piece", "secret".
8. **Every public type/function gets `///` doc comments.** Every `lib.rs` gets `//!` module docs. New modules → update that crate's `AGENTS.md` if one exists.
9. **Commit per slice** via `/sync`. Keep progress.md + sessions.md up to date so resumption is zero-friction.

## Architectural vision (load-bearing context)

### Identity model

**Today**: identity = one PQ keypair, encrypted at rest by a story-derived subkey.

**After Plan B**:
- **AccountRoot** = long-lived PQ keypair. `vk` published to peers via home-realm `_profile_identity` and pkarr. `sk` exists on-device only during initial setup; after steward split it's zeroized.
- **DeviceCertificate** = `{ device_vk, device_name, added_at, root_signature }`. Every device acting as the account has one, signed by root.
- **DeviceRoster** = `{ root_vk, devices, revoked }`, a CRDT doc in the home realm. Peers trust a device by verifying its cert against `root_vk`.
- **Pass story** = optional nostalgic fallback. Not primary auth.

### Trust cascade on recovery

1. New device generates a fresh PQ identity + display name. No ambient trust yet.
2. User contacts backup friends out-of-band ("I'm on a new phone, add me"). Stewards add the new device as a normal DM peer.
3. New device publishes `_recovery_request:{new_device_uid}` with its KEM ek into each steward's DM realm.
4. Each steward's UI shows "Alex is asking from a new device. Verify through phone/video/in-person first, then approve." No crypto burden.
5. K stewards approve. Each publishes `_share_release:{account_uid}` with their share re-wrapped to the new device's KEM ek.
6. New device polls, decrypts K shares, reassembles `AccountRoot.sk`, signs a new `DeviceCertificate`, publishes it into the `_device_roster` doc, **zeroizes `sk` immediately**.
7. Peers observe the new cert, verify signature against the known `root_vk`, auto-admit the new device to every shared realm. Profile, contacts, and shared-realm state replicate naturally.
8. Personal files (Plan C): new device walks a manifest, requests K-of-N Reed-Solomon shards from Backup Peers, reconstructs, re-encrypts to a new local vault key.

### Two distinct peer roles

| Role | Population | What they hold | Trust level |
|---|---|---|---|
| **Steward** | Few (K-of-N, K≈3, N≈5) | Shamir share of `AccountRoot.sk` | High — vouches for recovery |
| **Backup Peer** | Many (M-of-N, M≈3, N≈7) | Reed-Solomon shards of encrypted files | Low — holds opaque ciphertext |

Both groups are drawn from the user's DM-realm peer graph. Memberships can overlap but don't have to.

### Protocol doc keys (reference table)

| Key | Who writes | What it carries | Listener side effect |
|---|---|---|---|
| `_steward_invite:{acct_uid}` | User (enroller) | Display name + responsibility copy + K-of-N parameters + root-vk commitment | Steward UI: approve-role dialog |
| `_steward_accept:{acct_uid}` | Steward | `{ accepted, kem_ek_bytes, responded_at }` | Enroller counts quorum, splits root when ≥ N |
| `_steward_share:{acct_uid}` | User (after acceptance quorum) | `EncryptedStewardShare` wrapped to the steward's accepted KEM ek | Steward's device mirrors into local holdings cache |
| `_recovery_request:{new_device_uid}` | New device | `{ new_device_vk, display_name, new_device_kem_ek, account_uid, self_sig }` | Steward UI: approve-release dialog |
| `_share_release:{acct_uid}` | Steward (on approval) | Share re-wrapped to new device's KEM ek | New device polls + assembles |
| `_device_roster:{acct_uid}` | Any trusted device | `{ root_vk, devices[], revoked[] }` (CRDT, union-merge) | Peers trust new device certs |
| `_file_shard:{file_id}:{index}` | User (on file save) | `{ shard_bytes, per_file_key_envelope, metadata }` | Backup peer stores opaque ciphertext |

## Plan A — Invitation/release UX (current crypto) — target 1–2 weeks

Deliver the frictionless experience first. Keep pass-story–derived subkey as the Shamir secret. No root-authority work yet.

### Slice A.1 — invitation CRDT types + module
- New module `crates/indras-sync-engine/src/steward_enrollment.rs`.
- Types:
  - `StewardInvitation { from_uid, from_display_name, responsibility_text, threshold_k, total_n, issued_at_millis }`
  - `StewardResponse { accepted, responded_at_millis, kem_ek_bytes, dsa_vk_bytes }`
  - Doc-key helpers: `invite_doc_key(account_uid) -> _steward_invite:{hex}`, `response_doc_key(account_uid) -> _steward_accept:{hex}`.
- Impl `DocumentSchema` with last-writer-wins on `issued_at_millis` / `responded_at_millis`.
- Unit tests: key stability + merge + non-empty serialization.

### Slice A.2 — enrollment bridge
- `crates/synchronicity-engine/src/recovery_bridge.rs`:
  - `invite_steward(network, peer_uid, threshold_k, total_n) -> Result<(), String>` — writes `_steward_invite:{my_uid}` into the sender↔peer DM realm.
  - `revoke_invitation(network, peer_uid)` — writes a withdrawn-flavored invite (or zeros the doc).
  - `list_outgoing_enrollments(network) -> Vec<EnrollmentStatus>` — for user-side UI. Status enum: `NotInvited | Invited | Accepted | Declined | Withdrawn`.
  - `list_incoming_invitations(network) -> Vec<IncomingInvitation>` — for steward-side inbox.
  - `respond_to_invitation(network, from_uid, accept: bool) -> Result<(), String>` — publishes `_steward_accept:{from_uid}` with the steward's KEM ek.
- All reads use existing DM-realm scanning pattern from `refresh_held_backups`.

### Slice A.3 — Backup-plan overlay redesign (sender)
- Rewrite `crates/synchronicity-engine/src/components/recovery_setup.rs` end-to-end:
  - Section 01: **Your backup friends**. Peer list from DM realms (one entry per DM peer with a KEM key published). Each row: name, avatar, online dot, status badge (`Not invited` / `Invited — waiting` / `Accepted ✓` / `Declined`). Tap to invite/revoke.
  - Section 02: **How many must help** — K stepper. N derived from accepted count.
  - Section 03: **Plain-language explainer** of what being a backup friend means.
  - Section 04: **Backup status** — "Ready: 3 of 5 accepted ✓" or "Need 2 more acceptances".
  - **Delete**: hex picker, "Try with a fake friend" button, per-steward card with share/ek/dk fields, threshold "Make my backup" output textareas, story section 01, all `row.test_decap`, all `row.ek_hex` exposure.
  - No "Make my backup" explicit button — publication is automatic when quorum hits. User sees a success toast.
- Same file: keep `AvailableSteward` the input to the peer list but add `enrollment_status: EnrollmentStatus`.

### Slice A.4 — Steward inbox overlay (new component)
- New file `crates/synchronicity-engine/src/components/steward_inbox.rs`:
  - `StewardInboxOverlay` — opens from a new status-bar link `· Requests`.
  - Sections:
    - **Invitations** — pending steward role requests. Card per invitation: "{name} wants you as a backup friend. If they ever ask for help, you'll verify it's really them (phone/video/in person) before approving." Accept / Decline.
    - **You're a backup friend for** — accepted, no recovery in flight. Read-only list.
    - **Recovery requests** — (Slice A.6). Card per request: "{name} is asking for help from a new device. Verify through another channel first." Approve / Decline.
- Status-bar link shows badge count when any pending action exists.
- Mount alongside existing overlays in `home_vault.rs`.

### Slice A.5 — Share distribution on quorum
- Move share publication from "setup time" to "when N acceptances land".
- New bridge fn `recovery_bridge::finalize_steward_split(network, accepted_peers)`:
  - Called by an observer in the Backup-plan overlay when accepted count changes.
  - Pulls cached encryption subkey (or derives from story if present), splits K-of-N, publishes `_steward_share:{my_uid}` per accepted peer with their accepted-time KEM ek.
  - Idempotent — re-split on steward churn overwrites via last-writer-wins on the share doc's timestamp.
- Deprecate the old `setup_steward_recovery` tuple-input API; single entry-point is `finalize_steward_split`.

### Slice A.6 — Recovery request + release protocol
- New types in `steward_enrollment.rs` (or sibling module `recovery_protocol.rs`):
  - `RecoveryRequest { new_device_vk, new_device_kem_ek, display_name, account_uid, issued_at_millis, self_signature }`
  - `ShareRelease { steward_uid, encrypted_share: EncryptedStewardShare, approved_at_millis }`
  - Doc keys: `_recovery_request:{new_device_uid}`, `_share_release:{account_uid}` (in the steward↔new-device DM realm).
- Bridge fns:
  - `initiate_recovery(network, selected_steward_peer_uids) -> Result<(), String>` — publishes request docs in each selected DM realm.
  - `approve_recovery(network, request_uid)` — steward-side. Reads request, decrypts their own held share, re-encrypts to the requester's KEM ek, publishes `_share_release:*`.
  - `poll_recovery_releases(network, account_uid, threshold_k) -> RecoveryStatus { released_by, progress }` — new-device side.
  - `assemble_and_authenticate(network, account_uid, threshold_k) -> Result<(), String>` — when K releases are in, decrypt, recombine, re-auth keystore using the same flow Plan 1 built.

### Slice A.7 — Recovery overlay rewrite
- Rewrite `crates/synchronicity-engine/src/components/recovery_use.rs`:
  - **Delete** textareas for share/dk/ek hex. All of them.
  - Section 01: **Ask your backup friends for help** — peer list from DM realms (friends the new device has contacted). Checkbox per peer. Tap "Ask for help".
  - Section 02: **Waiting for approvals** — live progress per selected friend. "Waiting…", "✓ Approved 2m ago".
  - Section 03: **You're back in** — on K approvals, re-auth fires, overlay closes with a success toast.
- Reuse existing `.recovery-*` CSS.

### Slice A.8 — E2E tests
- `crates/indras-sync-engine/tests/steward_invitation_flow.rs` — two peers: user invites peer, peer accepts, user-side sees Accepted status within N iroh ticks.
- `crates/indras-sync-engine/tests/steward_recovery_flow.rs` — three peers (user, two stewards), user sets up, "user" goes offline, fresh "new-device" peer contacts stewards, issues recovery request, both approve, assembly succeeds.
- Use patterns from `tests/pq_signature_multi_peer.rs` for multi-node in-process iroh setup.

### Slice A.9 — Deprecations & /sync
- Delete `tests/steward_recovery_e2e.rs`'s hex-paste scenarios if they're now dead code. (Keep the crypto-level tests in `indras-crypto/src/shamir.rs` + `steward_share.rs`.)
- Delete `recovery_bridge::use_steward_recovery` (replaced by `assemble_and_authenticate`).
- Delete `recovery_bridge::generate_test_steward_keypair` — no more fake-friend hex.
- `/sync` with a clear summary commit.

### Plan A milestone commit message template
```
feat(A.N): <slice title>

<1–3 line body describing the slice.>
```

## Plan B — AccountRoot + DeviceCertificates — target 2–4 weeks

Load-bearing plan. Replaces pass-story–derived subkey as the secret Shamir protects. Devices become pluggable.

### Slice B.1 — `AccountRoot` primitive
- New module `crates/indras-crypto/src/account_root.rs`:
  - `AccountRoot { vk: PQVerifyingKey, sk: PQSigningKey }` — just uses the existing PQ signature primitive.
  - `generate() -> Self`, `verify(msg, sig, vk) -> bool`, `sign(&mut self, msg) -> Signature`.
  - `zeroize_sk` via `SecureBytes` wrapper.
  - Serialization helpers: `sk_to_bytes() -> SecureBytes` (for Shamir splitting), `sk_from_bytes(&[u8])` (for assembly).
- Unit tests: round-trip, sign-verify, zeroize-on-drop.

### Slice B.2 — `DeviceCertificate`
- New module `crates/indras-crypto/src/device_cert.rs`:
  - `DeviceCertificate { device_vk: Vec<u8>, device_name: String, added_at_millis: i64, revoked: bool, signature: Vec<u8> }`
  - `DeviceCertificate::sign(device_vk, name, added_at, root: &AccountRoot) -> Self`
  - `DeviceCertificate::verify(&self, root_vk: &PQVerifyingKey) -> bool`
- Unit tests.

### Slice B.3 — `DeviceRoster` CRDT doc
- New module `crates/indras-sync-engine/src/device_roster.rs`:
  - `DeviceRoster { account_uid, root_vk, devices: Vec<DeviceCertificate>, revoked: Vec<[u8; 32]> }`
  - `DocumentSchema::merge` = union of valid certs (verify each against `root_vk`); union of revoked lists.
  - Doc key: `_device_roster:{account_uid_hex}` in the home realm.
- Unit tests for merge with concurrent additions and revocations.

### Slice B.4 — Account creation flow
- `crates/synchronicity-engine/src/vault_bridge.rs::create_account`:
  - Generate `AccountRoot`. Publish `vk` to `_profile_identity` in home realm (new field `account_root_vk`).
  - Sign initial `DeviceCertificate` for this device's PQ identity. Publish via `_device_roster:{my_uid}`.
  - Stash the generated root in a Signal<Arc<Mutex<Option<AccountRoot>>>> on `AppState` for the duration of the session (needed for steward split).
  - No pass-story dependency in this path.
- Also update welcome / onboarding UX to guide user through "set up a backup friend now" — the root `sk` is only safe once K-of-N is split, so defer the zeroize until first quorum is reached.

### Slice B.5 — Root splitting at steward acceptance
- Replace `finalize_steward_split`'s secret input:
  - Old: pass-story subkey (from cache or derivation).
  - New: `AccountRoot.sk` bytes.
- After successful K-of-N split and publication of encrypted shares, zeroize the in-memory `AccountRoot.sk`. Keep the `vk` around (it's just public).
- UI: warning banner "You've set up your backup friends. Your recovery key is now only held by them." — reassuring copy.

### Slice B.6 — Recovery assembly on new device
- Rewrite `assemble_and_authenticate` (from A.6):
  - Decrypt K shares, reconstruct `AccountRoot.sk` bytes, wrap in `AccountRoot::sk_from_bytes`.
  - Sign a fresh `DeviceCertificate` for the new device's PQ identity.
  - Publish the new cert via `_device_roster:{account_uid}` (to the home realm the new device bootstraps once it knows `account_uid`).
  - **Immediately zeroize `AccountRoot.sk`** after the single signing operation.
  - Leave only: new device's own PQ identity + its now-signed cert.

### Slice B.7 — Peer verification of new devices
- `crates/indras-sync-engine/src/peer_verification.rs` (new):
  - `verify_device_against_roster(device_vk, roster) -> bool`.
- Wire into DM-realm admission: when a peer sees a new claim from `{account_uid}`, load that account's roster, check the claiming device has a valid cert.
- Revocation flow: a device's cert can be revoked by any trusted device's signature (not just root). Roster merges a revoked list.

### Slice B.8 — Pass-story deprecation
- Strip story from `create_account` path (done in B.4).
- `restore_account` becomes "restore via stewards" by default.
- Pass-story remains as a fallback-only opt-in flow for users who choose to have a 23-word backup password. Gated behind a "Use a secret phrase as extra backup" setting.
- Remove the auto-cache code in `vault_bridge::derive_encryption_subkey_for_restore` — no longer needed.

### Slice B.9 — E2E test
- `crates/indras-sync-engine/tests/account_root_recovery.rs` — full three-peer loop: create account (generates root, publishes roster), enroll 3 stewards, steward quorum splits root, "original device" goes offline, "new device" with fresh identity issues request, 2 stewards approve, new device assembles, signs cert, publishes. Old device's cert remains but new device cert verifies against root.

### Slice B.10 — Migration notes + /sync
- Document in `docs/migrations/account-root.md`: existing Phase-1 accounts (story-derived subkey stewards) don't have a root. Treat them as legacy-locked; option to "upgrade" by regenerating the account with a root (destroys old device cert identity; requires re-enrolling stewards).

## Plan C — Erasure-coded personal-data backup — target 3–5 weeks

Device loss becomes a hydration delay, not data loss.

### Slice C.1 — Reed-Solomon primitive
- Add `reed-solomon-erasure = "6"` to `indras-crypto/Cargo.toml`.
- New module `crates/indras-crypto/src/erasure.rs`:
  - `encode(data: &[u8], k: usize, n: usize) -> Vec<Vec<u8>>` — `n` shards total, any `k` reconstructs.
  - `decode(shards: Vec<Option<Vec<u8>>>, k: usize, n: usize) -> Result<Vec<u8>>`.
  - Unit tests: round-trip, missing-shard recovery, parameter validation.

### Slice C.2 — Backup-peer selection + config
- `crates/synchronicity-engine/src/backup_peers.rs` (new):
  - `BackupPeerPlan { file_id, k, n, peers: Vec<PeerAssignment> }`.
  - Selection algorithm: prefer DM peers with recent liveness (use `state.peer_liveness`), prefer peers who've accepted a "Backup Peer" role (new concept — simpler ack, no trust responsibility).
- UI: new Settings section "Backup peers" — show N peers currently holding shards, add/remove.

### Slice C.3 — File-shard CRDT
- New module `crates/indras-sync-engine/src/file_shard.rs`:
  - `FileShard { file_id: [u8; 32], shard_index: u8, shard_count: u8, k_threshold: u8, ciphertext: Vec<u8>, per_file_key_envelope: Vec<u8>, created_at_millis: i64 }`
  - `per_file_key_envelope` = per-file ChaCha20-Poly1305 key wrapped via ML-KEM to the peer's ek.
  - Doc key: `_file_shard:{file_id_hex}:{shard_index}` in the owner↔peer DM realm.
  - Impl `DocumentSchema` with last-writer-wins on `created_at_millis` (re-save overwrites).

### Slice C.4 — Publish-on-save hook
- Hook into vault file-save:
  - On every file write, compute content-addressed hash (already in blob_store).
  - Generate per-file ChaCha20 key, encrypt file bytes, Reed-Solomon encode ciphertext into N shards.
  - Wrap per-file key for each Backup Peer's KEM ek.
  - Publish one `_file_shard:*` per peer.
- Batched so a bulk edit doesn't flood iroh.
- Removal: on file-delete, publish empty/tombstone shard docs (or let the existing doc become superseded by a later version).

### Slice C.5 — Recovery-time re-pull
- After Plan B's identity cascade:
  - New device's `recovery_bridge` walks every DM realm, collects all `_file_shard:{file_id}:*` doc keys grouped by `file_id`.
  - For each file: decrypt the per-file key envelope using its own KEM dk, wait for K shards, Reed-Solomon decode, decrypt, write to local vault.
- UI: progress overlay "Restoring 47 files from your backup friends — 12/47 complete".

### Slice C.6 — Shared-realm re-join
- Once new device cert is in rosters, peers auto-invite the new device to every shared realm they have with the account.
- New device ingests realm state normally via CRDT sync.
- No explicit action needed — this is an emergent property of device-cert cascade + existing realm admission logic.

### Slice C.7 — E2E test
- `crates/indras-sync-engine/tests/data_recovery_flow.rs` — 4 peers (user, 3 backup peers). User saves 5 files, each sharded 2-of-3. "User" goes offline. New device with fresh identity joins, pulls shards, reconstructs files. Verify byte-for-byte match.

### Slice C.8 — Polish, docs, /sync
- Document the full flow in `articles/recovery-architecture.md`.
- Update `DESIGN.md` plain-language UI copy tables.

## Critical files (code map)

Existing — read before editing:
| File | Role |
|---|---|
| `crates/indras-crypto/src/shamir.rs` | K-of-N primitive (Plan A+B rely on it; unchanged) |
| `crates/indras-crypto/src/steward_share.rs` | `EncryptedStewardShare` + ML-KEM envelope |
| `crates/indras-sync-engine/src/share_delivery.rs` | `ShareDelivery` CRDT + DM-realm doc key helpers (Plan A reuses for `_steward_share:*`; new docs add alongside) |
| `crates/indras-sync-engine/src/peer_key_directory.rs` | `PeerKeyDirectory` doc (KEM keys per realm) |
| `crates/indras-network/src/network.rs` | `conversation_realms`, `dm_peer_for_realm`, `get_realm_by_id` |
| `crates/indras-network/src/document.rs` | `DocumentSchema` trait; last-writer-wins patterns |
| `crates/synchronicity-engine/src/recovery_bridge.rs` | Existing backup/recovery glue; Plan A rewrites ~half of it |
| `crates/synchronicity-engine/src/components/recovery_setup.rs` | Backup-plan overlay — Plan A rewrites fully |
| `crates/synchronicity-engine/src/components/recovery_use.rs` | Recovery overlay — Plan A rewrites fully |
| `crates/synchronicity-engine/src/components/status_bar.rs` | Status-bar links (+ new `· Requests` for steward inbox) |
| `crates/synchronicity-engine/src/components/home_vault.rs` | Overlay mount point |
| `crates/synchronicity-engine/src/profile_bridge.rs` | `load_peer_profile_from_dm` and `_profile_identity` write path — Plan B adds `account_root_vk` |

New in this plan:
| File | Slice | Role |
|---|---|---|
| `crates/indras-sync-engine/src/steward_enrollment.rs` | A.1 | Invitation/response CRDTs |
| `crates/synchronicity-engine/src/components/steward_inbox.rs` | A.4 | Inbox overlay for pending invitations + recovery requests |
| `crates/indras-sync-engine/src/recovery_protocol.rs` | A.6 | `RecoveryRequest` + `ShareRelease` CRDTs |
| `crates/indras-sync-engine/tests/steward_invitation_flow.rs` | A.8 | Invitation/accept E2E |
| `crates/indras-sync-engine/tests/steward_recovery_flow.rs` | A.8 | Full recovery-request E2E |
| `crates/indras-crypto/src/account_root.rs` | B.1 | `AccountRoot` PQ keypair + zeroize |
| `crates/indras-crypto/src/device_cert.rs` | B.2 | `DeviceCertificate` + verification |
| `crates/indras-sync-engine/src/device_roster.rs` | B.3 | Roster CRDT in home realm |
| `crates/indras-sync-engine/src/peer_verification.rs` | B.7 | Device cert verification on peer side |
| `crates/indras-sync-engine/tests/account_root_recovery.rs` | B.9 | Full root-based recovery E2E |
| `crates/indras-crypto/src/erasure.rs` | C.1 | Reed-Solomon encode/decode |
| `crates/synchronicity-engine/src/backup_peers.rs` | C.2 | Backup-peer selection + plan |
| `crates/indras-sync-engine/src/file_shard.rs` | C.3 | File shard CRDT |
| `crates/indras-sync-engine/tests/data_recovery_flow.rs` | C.7 | End-to-end file recovery |

Deprecated in this plan:
| Area | Fate |
|---|---|
| Hex-paste share input (current `recovery_use.rs`) | Deleted in A.7 |
| "Try with a fake friend" + `generate_test_steward_keypair` | Deleted in A.3/A.9 |
| Pass-story subkey cache (`story.subkey` file) | Removed in B.8 |
| `RecoveryContribution` struct, `use_steward_recovery` bridge fn | Deleted in A.9 |

## Verification strategy

- `cargo build -p indras-crypto`, `-p indras-sync-engine`, `-p synchronicity-engine` must be clean after every slice.
- `cargo test -p indras-crypto`, `-p indras-sync-engine`, `-p synchronicity-engine` — scoped.
- **Never `cargo test` unscoped.**
- Each slice's E2E test lives in `tests/` and runs in the scoped crate test.
- Manual UX pass at the end of each Plan — `cargo run -p synchronicity-engine`, walk through the flow, assert plain-language copy, no hex exposed.
- Multi-peer E2E patterns borrowed from `tests/pq_signature_multi_peer.rs`.

## Out of scope (for this plan)

- **Threshold signature schemes** (FROST, threshold BLS, etc.). Would eliminate the brief moment where the new device holds reassembled `sk`. PQ-compatible threshold sig primitives aren't production-ready; revisit in a separate post-Plan-C plan.
- **Hardware-key fallback**. Option to back up root to a YubiKey / Secure Enclave. Nice-to-have; defer.
- **UI for steward health monitoring**. ("Is your backup friend still online? Your backup is at risk." heartbeat-based warnings.) Phase 4.
- **Account merging / multi-account support**. Phase 4.
- **Braid sync app wiring** — agent1 owns `brisk-orbiting-lantern`; do not touch `ipc.rs`, `sync_panel.rs`, `vault/sync_all.rs`, `vault/view_models.rs`, `vault_manager.rs`'s GC loop.

## Open questions to surface as they arise

- **Revocation signer policy**: device revocations signed by root only (requires quorum), or by any trusted device (faster, weaker)? — Default to any-trusted-device; root is reserved for genesis moments.
- **Root VK discovery**: home-realm `_profile_identity` + pkarr only, or also embedded in DM-realm peer-keys? — Default to `_profile_identity` only; pkarr as secondary.
- **Onboarding flow when user has no peers yet**: require a peer before account is usable, or allow a "quorum of one = me" warning? — Default to warning + require within 7 days.
- **Re-split cadence when a steward churns**: automatic on next app start, or explicit user action? — Default to explicit, with a banner prompt.
- **Backup-peer acceptance**: does becoming a Backup Peer need explicit accept like steward, or is it opt-out? — Default to opt-in via a simpler "You can help store files for your friends — turn on" toggle. Lighter than steward role.

## Pointers

- Parent plan: `~/.claude/plans/i-have-a-kind-lexical-breeze.md` (do not edit — sibling agents may read).
- Prior work that this plan replaces: `./plans/steward-backup-completion.md` (reference only; all slices completed upstream through commit `45972d56`).
- Design system: `DESIGN.md` (plain language, no crypto-algorithm names in UI).
- Community identity article: `articles/the-heartbeat-of-community.md`.
- Developer guide: `articles/indras-network-developers-guide.md`.
- This worktree: `/Users/truman/Code/IndrasNetwork/agent3`.
