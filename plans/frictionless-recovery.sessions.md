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
