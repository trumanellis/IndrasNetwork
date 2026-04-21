# Migration: Phase-1 pass-story accounts → Plan-B AccountRoot

Plan-B introduces a first-class `AccountRoot` + `DeviceRoster`
architecture (see `plans/frictionless-recovery.md`). Phase-1
accounts — which used the pass-story-derived encryption subkey as
the Shamir secret — don't have a root on disk. This note describes
what happens to them and what the upgrade path looks like.

## What "legacy account" means

A Phase-1 account has these artifacts:

- `<data_dir>/story.token` — verification token (32 bytes).
- `<data_dir>/story.salt` — salt used for key derivation.
- `<data_dir>/story.subkey` — optionally cached 32-byte subkey.
- `<data_dir>/steward_recovery.json` — Shamir manifest from a prior
  `setup_steward_recovery` call (if the user had already set up
  stewards before Plan B landed).
- Plaintext PQ identity files (`pq_signing`, `pq_verifying`, KEM
  keypair).

It **does not** have:

- `<data_dir>/account_root.pending` (Plan-B cache).
- Any `_device_roster` or `_account_root_envelope` doc in its home
  realm.

## Runtime behavior without migration

Both the Backup-plan and Recovery overlays support the legacy path
as a fallback:

- `finalize_steward_split` prefers the pending AccountRoot, falls
  back to `load_subkey_cache(&data_dir)`. So a legacy account can
  still enroll stewards as long as `story.subkey` is present on
  disk or the user has signed in this session.
- `assemble_and_authenticate` tries the AccountRoot envelope first
  and falls through to the legacy `StoryKeystore::authenticate`
  path when the envelope is missing or un-unsealable.

This means Phase-1 accounts **continue to work** — UX-wise they
behave the same, just without the device-roster / cross-device
story.

## Known limitations

- **No cross-device recovery.** The legacy path re-auths the on-
  disk keystore; it can't issue a fresh device certificate because
  there's no root to sign with. "Recover on a new device" requires
  Plan-B or a future opt-in migration.
- **No device roster.** Shared-realm peers have no way to verify
  "this new device really belongs to account X" without a root-
  signed cert. Admission decisions on legacy accounts fall back to
  whatever the realm layer does today (typically: first-writer-
  wins, with no attestation).

## Upgrade options

### Option 1 — Regenerate account (destructive)

The simplest: the user recreates their account, which takes the
Plan-B path automatically via `create_account` / `bootstrap_account_root`.
This loses the prior PQ identity (and any existing memberships in
shared realms). Acceptable for dev/test environments.

### Option 2 — Inline bootstrap at first Plan-B action (future)

A later slice can add a one-time flow: when the user opens the
Backup-plan overlay on a legacy account, prompt them to confirm,
then generate a fresh `AccountRoot`, re-sign the existing device's
cert, publish the `DeviceRoster` + envelope, and tear down
`story.subkey`. The PQ identity stays the same — only the
attestation layer changes.

This flow is deliberately deferred until we have telemetry on how
many real legacy accounts exist and a UX sketch that makes the
one-time action feel like a routine setting change, not a scary
migration.

### Option 3 — Hardware-key (future)

Plan 3+ may introduce passkey / Secure Enclave attestation as an
alternative root-protection story. A legacy account on a device
with a passkey could bind to Plan B without network-side steward
involvement.

## Implementor checklist

- Don't delete legacy files in `finalize_steward_split` / `assemble_and_authenticate`
  — they're the fallback. Removal belongs to a future cleanup slice.
- Always call `account_root_cache::has_pending_root(&data_dir)` before
  assuming the legacy path is the one to use.
- When writing tests that exercise legacy behavior, seed
  `story.token` + `story.salt` + PQ identity files directly; the
  `StoryAuth::create_account` helper is the easiest way to do that.
