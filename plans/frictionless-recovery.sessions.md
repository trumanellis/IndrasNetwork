# Sessions: Frictionless Recovery

## 2026-04-21 — plan drafted
- Session focus: reinvent the Shamir/steward flow around the user's "frictionless" vision.
- Deep-reasoned the architectural shift: logical account + pluggable devices, AccountRoot + DeviceCertificates, stewards-as-human-verifiers, erasure-coded personal data.
- Three-plan arc:
  - **Plan A**: invitation/release UX over DM realms. Keep current crypto (pass-story subkey). Deliver the visible UX win first.
  - **Plan B**: AccountRoot + DeviceCertificates. Replace subkey as the Shamir secret; pass-story becomes optional.
  - **Plan C**: Reed-Solomon personal-data backup + cross-device re-hydration.
- Slice-level decomposition written into `./plans/frictionless-recovery.md` with code map, verification strategy, out-of-scope, open questions.
- Predecessor plan (`steward-backup-completion.md`) is complete upstream (commits `a84131e…45972d56`); this plan rewrites the UX parts end-to-end and leaves the crypto primitives (`shamir.rs`, `steward_share.rs`) in place.
- Next: Slice A.1 — draft the invitation/response CRDT module with unit tests.

## 2026-04-21 — Plan A landed end to end
Completed slices (one commit each via `/sync`):
- **A.1** `fbcc7a65` — `steward_enrollment` CRDT module (StewardInvitation + StewardResponse + EnrollmentStatus; 5 unit tests).
- **A.2** `d49754ef` — enrollment bridge fns (invite_steward / revoke_invitation / list_outgoing_enrollments / list_incoming_invitations / respond_to_invitation) with DM-only scanning and label resolution.
- **A.3** `35359d1f` — Backup-plan overlay rewrite: peer list with Ask/Remove + per-row status badges + K stepper + plan summary. Deleted hex picker, fake-friend button, pass-story manuscript.
- **A.4** `22796c31` — Steward inbox overlay + status-bar `· Requests` link + `steward_inbox_pending` badge + accept/decline per card.
- **A.5** `84fa0dd9` — auto-finalize split when accepted fingerprint changes and size >= K; shares flow through existing `_steward_share:*` doc.
- **A.6** `009d06e0` — `recovery_protocol` CRDT + bridge fns (`initiate_recovery`, `approve_recovery_request`, `poll_recovery_releases`, `assemble_and_authenticate`).
- **A.7** `bafa8733` — Recovery overlay rewrite (Picking → Waiting → Unlocking → Done) with live-approval polling + steward-inbox section 03 recovery-request approvals.
- **A.8** deferred — a multi-peer DM-realm E2E harness doesn't exist yet; unit coverage stands. Flag: build `indras-network/tests/direct_connect_harness.rs` before landing `steward_invitation_flow.rs` / `steward_recovery_flow.rs`.
- **A.9** `_this commit_` — deleted `setup_steward_recovery`, `StewardInput`, `SetupOutcome`, `dm_realm_map`, `generate_test_steward_keypair`, `RecoveryContribution`, `use_steward_recovery`; rewrote module docstring.

Final state: Plan A UX complete. User sees peer list with names + live status, never touches hex. `cargo build -p synchronicity-engine -p indras-sync-engine` clean; `cargo test -p indras-sync-engine --lib` 314/314. Plan-A limitation: recovery's source_account_uid auto-matches the DM-peer UID (same-device recovery). True cross-device recovery follows Plan B.

Next: Slice B.1 — `AccountRoot` primitive (PQ keypair with zeroize-on-drop).

## 2026-04-21 — Plan B landed end to end
Completed slices:
- **B.1** `97a84391` — `AccountRoot` primitive + `AccountRootRef` snapshot type (5 tests).
- **B.2** `ed17fc75` — `DeviceCertificate` with sign / verify / revoke + domain-separated canonical message (5 tests).
- **B.3** `2353be6d` — `DeviceRoster` CRDT doc in home realm with per-device upsert + `device_is_trusted` (4 tests).
- **B.4** `a969f927` — `vault_bridge::create_account` now calls `bootstrap_account_root`: generate root → sign initial cert → publish roster → stash root sk in `account_root_cache` for the first split (1 test).
- **B.5** `0d69f25d` — `AccountRootEnvelope` module (ChaCha20-Poly1305 over root sk) + `finalize_steward_split` now seals root under a random 32-byte wrapping key, publishes envelope to home realm, Shamir-splits the wrapping key across stewards, clears pending cache on quorum delivery (3 tests).
- **B.6** `05c2a99a` — `assemble_and_authenticate` prefers the Plan-B path: unseal envelope → sign fresh `DeviceCertificate` → upsert into roster → drop root. Legacy keystore re-auth kept as fallback.
- **B.7** `23accc54` — `peer_verification` helpers (`verify_peer_device`, `load_device_roster`) for gating peer admission against an account's roster. Network-layer wiring of these checks remains a follow-up.
- **B.8** `a6023ec4` — Welcome screen reframed: pass-story is secondary; Use-backup is the primary cross-device recovery path.
- **B.9** `61d0d499` — full crypto-level E2E `tests/account_root_recovery.rs` (generate → seal → split → rewrap → reassemble → unseal → sign cert → verify). 2 tests pass including attacker-forged-root rejection.
- **B.10** `_this commit_` — migration notes in `docs/migrations/account-root.md` covering legacy-account compat + upgrade options.

Security posture: root sk is on disk in `account_root.pending` only between account creation and first successful split; after that it's gone locally and only exists as Shamir shares distributed to stewards. Envelope ciphertext in home realm + wrapping key held by stewards is the durable storage.

Next: Slice C.1 — Reed-Solomon primitive for personal-data backup.
