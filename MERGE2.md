# Merge: Intention Game Loop with Peer Sync

Branch `feature/intention-game-loop` (worktree: `IndrasNetwork-intention-game-loop`)

## What Changed

Intention artifact type, creation/sharing UI, and peer-to-peer sync via CRDT realm invites. **52 files changed, +4169 -2559 lines.**

### Backend (indras-artifacts)

- **intention.rs** (new) — `Intention` artifact with audience list and description leaf
- **vault.rs** — Intention creation via `Intention::create()`, vault composition
- **attention.rs** — Attention tracking additions
- **artifact.rs / store.rs / lib.rs** — Extended artifact primitives for intention support

### Backend (indras-network)

- **chat_message.rs** — `RealmInvite` message type for DM-based realm sharing
- **home_realm.rs** — `grant_access()` used with realm artifact_id for correct sync
- **artifact_index.rs** — Extended artifact index queries
- **document.rs** — Simplified document schema

### UI (indras-workspace)

- **app.rs** — Intention creation flow, audience selection, realm sharing with `grant_access` + DM invite, persistent DM realm cache for reliable CRDT sync
- **quest.rs** — `IntentionView`, `IntentionCreateOverlay`, attention/token UI components
- **event_log.rs** — Event log updates for intention lifecycle
- **workspace.css** — Intention and quest UI styles
- **vault_bridge.rs** — Bridge extensions for intention data

### Key Bug Fixes (latest commit)

- **grant_access artifact_id** — Was using local vault `intention.id` instead of `realm.artifact_id()`, causing sync interfaces to track the wrong ID
- **DM realm persistence** — `net.connect()` returns fresh `Realm` each call; now cached in `HashMap` so CRDT Document listeners survive across polling iterations

### Deleted

- **artifact_browser.rs** — Replaced by intention-focused UI
- **fresh_mock.lua / mock_artifacts.lua** — Lua scenarios removed
- **lua_runtime.rs bindings** — Simplified scripting layer

## Merge Steps

```bash
cd /Users/truman/Code/IndrasNetwork

# 1. Merge
git merge feature/intention-game-loop

# 2. Verify
cargo build -p indras-artifacts
cargo test -p indras-artifacts
cargo build -p indras-network
cargo test -p indras-network
cargo check -p indras-workspace

# 3. Clean up worktree
git worktree remove ../IndrasNetwork-intention-game-loop

# 4. Delete the branch
git branch -d feature/intention-game-loop
```

## If Not Fast-Forward

```bash
# Option A: Merge (preserves history)
git merge feature/intention-game-loop

# Option B: Rebase branch first (linear history)
git checkout feature/intention-game-loop
git rebase main
git checkout main
git merge feature/intention-game-loop
```

Conflicts would likely be in:
- `crates/indras-workspace/src/components/app.rs` — large refactor, most likely conflict point
- `crates/indras-network/src/home_realm.rs` — if HomeRealm methods were added on main
- `crates/indras-artifacts/src/vault.rs` — if vault API changed on main
- `crates/indras-network/src/document.rs` — if document schema changed
- `se` — if launch script was modified

## Test Summary

- indras-workspace builds cleanly (`cargo check -p indras-workspace`)
- No compilation errors introduced

All pass as of commit `e5e034b`.
