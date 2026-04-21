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
