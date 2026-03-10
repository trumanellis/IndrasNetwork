# Merge Queue

## Completed

- **feature/artifact-browser** — merged into main (2026-02-22). 3-column artifact browser UI, artifact detail modal, navigation sidebar, audience popup. Worktree removed, branch deleted.
- **feature/telegram-chat** — merged into main (2026-02-22). New `indras-chat` crate with P2P Telegram-style chat, replaced workspace StoryView with embedded ChatLayout. Worktree removed, branch deleted. Conflict in app.rs resolved (import merge + `infer_artifact_type` re-applied).

## Pending: `worktree-relay-node`

Branch `worktree-relay-node` (worktree: `.claude/worktrees/relay-node`)

### What Changed

Evolves `indras-relay` from a blind store-and-forward relay into an authenticated three-tier relay node with credential-based auth and per-tier storage staging. **12 files changed, +1663 -141 lines.**

### New Files

- **`crates/indras-relay/src/auth.rs`** — `AuthService`: Ed25519 credential validation, session tracking, tier assignment. Credential format: `{ player_id, transport_pubkey, expires_at }` signed with player's Ed25519 key.
- **`crates/indras-relay/src/tier.rs`** — Tier determination logic: `determine_tier(player_id, owner_id, contacts) → StorageTier`, `granted_tiers()`, per-tier config helpers.

### Modified Files

- **`crates/indras-transport/src/protocol.rs`** — Added `StorageTier` enum, 4 new `WireMessage` variants (`RelayAuth`, `RelayAuthAck`, `RelayStore`, `RelayStoreAck`), 6 supporting structs. Added `PartialEq` to all protocol types.
- **`crates/indras-relay/src/config.rs`** — Added `TierConfig` (per-tier quotas/TTLs), `owner_player_id`, `community_mode` fields.
- **`crates/indras-relay/src/error.rs`** — Added `AuthenticationFailed`, `TierAccessDenied`, `InvalidCredential` variants.
- **`crates/indras-relay/src/blob_store.rs`** — 6 redb tables (2 per tier), tiered store/retrieve/cleanup/usage methods.
- **`crates/indras-relay/src/quota.rs`** — Added `TieredQuotaManager` with per-tier byte and interface limits.
- **`crates/indras-relay/src/relay_node.rs`** — Auth flow, tier-aware dispatch, `RelayStore` handler, per-tier cleanup TTLs.
- **`crates/indras-relay/src/admin.rs`** — Per-tier stats, peer tier access in admin API.
- **`crates/indras-relay/src/lib.rs`** — Updated module docs, added `auth` and `tier` modules.
- **`crates/indras-relay/Cargo.toml`** — Added `ed25519-dalek` dependency.

### Merge Steps

```bash
cd /Users/truman/Code/IndrasNetwork

# 1. Merge (already rebased onto origin/main — should fast-forward)
git merge worktree-relay-node

# 2. Verify
cargo test -p indras-relay -p indras-transport
cargo build -p indras-relay

# 3. Push
git push

# 4. Clean up worktree
git worktree remove .claude/worktrees/relay-node

# 5. Delete the branch
git branch -d worktree-relay-node
git push origin --delete worktree-relay-node
```

### If Not Fast-Forward

```bash
# Rebase first (should already be current)
git checkout worktree-relay-node
git rebase main
git checkout main
git merge worktree-relay-node
```

Conflicts would likely be in:
- `crates/indras-transport/src/protocol.rs` — if new WireMessage variants were added on main
- `Cargo.lock` — dependency resolution (auto-resolvable)

### Test Summary

- `cargo test -p indras-relay` — 40 tests pass (23 new + 17 existing)
- `cargo test -p indras-transport` — 62 tests pass (4 new + 58 existing)
- `cargo build -p indras-relay` — zero errors, zero warnings

All pass as of commit `ac3efdb` (rebased onto origin/main).
