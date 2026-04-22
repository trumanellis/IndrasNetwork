# Progress: Frictionless Recovery

## Completed
- [x] Plan drafted 2026-04-21

## Pending

### Plan A — Invitation/release UX

- [x] A.1 — invitation CRDT types + module (`steward_enrollment.rs` + unit tests) — 5/5 pass
- [x] A.2 — enrollment bridge (`invite_steward`, `revoke_invitation`, `list_outgoing_enrollments`, `list_incoming_invitations`, `respond_to_invitation`) — builds clean
- [x] A.3 — Backup-plan overlay rewrite (peer list with per-row status badges; delete hex / story / fake-friend flows) — builds clean, styles added
- [x] A.4 — Steward inbox overlay (`components/steward_inbox.rs`; status-bar `· Requests` link + badge) — builds clean
- [x] A.5 — Share distribution on quorum (`finalize_steward_split` + overlay auto-trigger by accepted-set signature) — builds clean
- [x] A.6 — Recovery request + release protocol (`recovery_protocol.rs` + bridge fns: `initiate_recovery`, `withdraw_recovery_request`, `list_incoming_recovery_requests`, `approve_recovery_request`, `poll_recovery_releases`, `assemble_and_authenticate`) — 3 unit tests pass
- [x] A.7 — Recovery overlay rewrite + steward-inbox recovery-request section (Plan-A uses same-device auto-match for source account; true new-device lands with Plan B's AccountRoot)
- [~] A.8 — E2E tests **deferred**. Unit coverage landed per module (`steward_enrollment` 5/5, `recovery_protocol` 3/3, `share_delivery` 3/3). Multi-peer DM-realm E2E needs a `DirectConnect`-based test harness that doesn't exist yet in `indras-network/tests/`; follow-up slice will build that harness and port the `steward_invitation_flow` / `steward_recovery_flow` scenarios onto it.
- [x] A.9 — Deprecations + /sync — dropped `setup_steward_recovery`, `StewardInput`, `SetupOutcome`, `dm_realm_map`, `generate_test_steward_keypair`, `RecoveryContribution`, `use_steward_recovery`. Module docstring rewritten around the new flow. Build clean; `indras-sync-engine --lib` 314/314.

### Plan B — AccountRoot + DeviceCertificates

- [x] B.1 — `AccountRoot` primitive (`indras-crypto/src/account_root.rs`) + `AccountRootRef` — 5/5 tests pass
- [x] B.2 — `DeviceCertificate` (`indras-crypto/src/device_cert.rs`) with sign / verify / revoke + domain-separated canonical message — 5/5 tests pass
- [x] B.3 — `DeviceRoster` CRDT doc (`indras-sync-engine/src/device_roster.rs`) with per-device upsert + device_is_trusted — 4/4 tests pass
- [x] B.4 — Account creation generates `AccountRoot`, signs first `DeviceCertificate`, publishes `DeviceRoster` in home realm, caches pending root sk for B.5 split (`account_root_cache` + `bootstrap_account_root` helper in vault_bridge) — 1 cache test passes
- [x] B.5 — `finalize_steward_split` seals the pending root under a fresh 32-byte wrapping key, publishes `_account_root_envelope` in home realm, Shamir-splits the wrapping key across stewards, and clears the pending cache once quorum lands. Legacy story-subkey path retained as fallback. 3 envelope tests pass.
- [x] B.6 — `assemble_and_authenticate` unseals the `_account_root_envelope` with the reassembled wrapping key, signs a fresh `DeviceCertificate` for this device, upserts into the `DeviceRoster`, and drops the root. Legacy keystore path retained as fallback. Build clean.
- [x] B.7 — `peer_verification::verify_peer_device` / `load_device_roster` helpers for gating device admission against an account's `DeviceRoster`. Wiring into shared-realm admission logic remains for a follow-up (network-layer change).
- [x] B.8 — Welcome UX reframed: pass-story sign-in is now the secondary option; hint points users to `· Use backup` in the status bar for cross-device recovery. Legacy story-subkey cache kept as in-memory fallback for accounts that pre-date Plan B. Build clean.
- [x] B.9 — `tests/account_root_recovery.rs` crypto-level E2E: generate root → envelope-seal → Shamir-split → release-rewrap → reassemble → sign fresh device cert → verify against roster. 2 tests pass (`full_plan_b_recovery_cycle`, `below_threshold_cannot_unseal_envelope`). Deferred: network-layer E2E needs a DM-realm harness, same as A.8.
- [x] B.10 — Migration notes in `docs/migrations/account-root.md` + /sync. Plan B complete.

### Plan C — Erasure-coded personal-data backup

- [x] C.1 — Reed-Solomon primitive (`indras-crypto/src/erasure.rs`) with `encode` / `decode` + padding-aware original-length tracking — 6 tests pass
- [x] C.2 — Backup-peer role CRDT + selection (`backup_peers.rs`) with `BackupPeerAssignment` doc schema, `select_top` ranking (online > alphabetical), and plain-language responsibility copy — 4 tests pass
- [x] C.3 — `FileShard` CRDT (`file_shard.rs`) + `prepare_file_shards` / `reconstruct_file` helpers. Two-layer encryption: per-file ChaCha20-Poly1305 key, itself wrapped under the AccountRoot wrapping key W. Erasure-coded ciphertext travels in one doc per peer. 6 tests pass.
- [x] C.4 — `publish_vault_backup` bridge + Backup-plan overlay "Back up my files now" button. MVP uses user-triggered publish rather than a notify-based save-hook; re-publishing is safe via last-writer-wins on shard docs + the new `FileBackupIndex` home-realm CRDT.
- [x] C.5 — `repull_vault_backup` bridge auto-fires after `assemble_and_authenticate` succeeds; the Recovery overlay renders "Restored N files / M still missing" progress. Reads `_file_backup_index` to learn which `file_id`s to chase.
- [~] C.6 — Shared-realm re-join **deferred**. Emergent from B.7 once peer admission actually consults `DeviceRoster::device_is_trusted`; the roster primitive is shipped and tested, admission wiring is a network-layer follow-up.
- [x] C.7 — `tests/data_recovery_flow.rs` crypto-level E2E covers multi-file publish, parity-budget loss survival, below-threshold failure, and wrong-wrapping-key rejection. 3 tests pass. Network-level E2E still gated on the DM-realm harness gap flagged in A.8 / B.9.
- [x] C.8 — Architecture doc at `articles/recovery-architecture.md` describing the full A → B → C pipeline; `/sync` broadcasts plan closure.

## Notes
- Agent3 worktree, branch `agent3`. Sibling peers: agent1 (braid backend), agent2 (braid UI). Hands off their lanes.
- Use `/sync`, not manual git. Stash unrelated dirty state before rebase.
- Scoped `cargo test -p <crate>` only.
- Plain-language UI copy, no crypto-algorithm names.
- Each slice ends in a `/sync` commit + progress.md tick + sessions.md entry.
