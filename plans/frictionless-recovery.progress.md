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
- [ ] B.5 — Root splitting at steward acceptance (replaces pass-story subkey)
- [ ] B.6 — Recovery assembly on new device signs fresh device cert
- [ ] B.7 — Peer verification of new device certs against roster
- [ ] B.8 — Pass-story deprecation; strip `story.subkey` cache
- [ ] B.9 — E2E test (`tests/account_root_recovery.rs`)
- [ ] B.10 — Migration notes + /sync

### Plan C — Erasure-coded personal-data backup

- [ ] C.1 — Reed-Solomon primitive (`indras-crypto/src/erasure.rs`)
- [ ] C.2 — Backup-peer selection + config (`backup_peers.rs`)
- [ ] C.3 — `FileShard` CRDT (`indras-sync-engine/src/file_shard.rs`)
- [ ] C.4 — Publish-on-save hook
- [ ] C.5 — Recovery-time re-pull
- [ ] C.6 — Shared-realm re-join (emergent from B.7; verify)
- [ ] C.7 — E2E test (`tests/data_recovery_flow.rs`)
- [ ] C.8 — Polish, docs, /sync

## Notes
- Agent3 worktree, branch `agent3`. Sibling peers: agent1 (braid backend), agent2 (braid UI). Hands off their lanes.
- Use `/sync`, not manual git. Stash unrelated dirty state before rebase.
- Scoped `cargo test -p <crate>` only.
- Plain-language UI copy, no crypto-algorithm names.
- Each slice ends in a `/sync` commit + progress.md tick + sessions.md entry.
