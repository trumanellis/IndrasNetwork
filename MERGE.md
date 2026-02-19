# Merge: Artifact Browser with Lua-Scripted Seeding

Branch `feature/artifact-browser` (worktree: `IndrasNetwork-artifact-browser`)

## What Changed

3-column artifact browser (list / detail / map), Lua-scripted artifact creation through real HomeRealm API with BLAKE3 content-addressed IDs, `grant_access()` for P2P sync, `--remock` flag for clean-start with auto-connect. **42 files changed, +3108 -1253 lines.**

### Backend (indras-network, indras-artifacts)

- **artifact_index.rs** — `GeoLocation`, `ArtifactProvenance`, tree composition (`attach_children`, `detach`), `active_artifacts()` / `active_count()` / `accessible_by()` queries, `grant` / `revoke_access` / `recall` / `transfer` operations
- **access.rs** — `AccessGrant`, `AccessMode`, `GrantError`, `RevokeError`, `TransferError`, `TreeError` types
- **home_realm.rs** — `grant_access`, `revoke_access`, `recall`, `transfer`, `ensure_dm_story`, `ensure_realm_artifact`, `shared_with`, `attach_children/child`, `detach_all_children/child` methods; `ArtifactSyncRegistry` reconciliation on grant changes
- **artifact.rs / store.rs / vault.rs** — Reworked artifact primitives (indras-artifacts crate)
- **document.rs** — Simplified document schema

### Scripting (indras-workspace/scripting)

- **action.rs** — `StoreArtifact`, `GrantArtifact`, `SetUserLocation`, `ConnectToPeer` actions
- **dispatcher.rs** — `StoreArtifact` uses `home.share_artifact()` for real BLAKE3 IDs; `GrantArtifact` calls `home.grant_access()` with `AccessMode::Revocable`; deadlock fix (each dispatcher owns its receiver directly, no shared mutex)
- **event.rs** — `ArtifactStored`, `ArtifactGranted`, `PeerConnected` events
- **query.rs** — `ArtifactCount`, `Artifacts`, `IdentityUri` queries
- **lua_runtime.rs** — `store_artifact`, `grant_artifact`, `connect_to`, `file_write`, `file_read`, `set_user_location` Lua bindings
- **channels.rs** — `Option<Receiver>` pattern for `.take()` extraction (avoids mutex deadlock)

### UI (indras-workspace/components)

- **artifact_browser.rs** (new) — 3-column layout: filterable list (MIME chips + peer filter), detail panel (metadata + location), Leaflet map with markers
- **app.rs** — Artifact loading from `HomeRealm::artifact_index()`, `origin_label` derivation from provenance, peer filter signal, `--seed-script` support, auto-identity creation via `INDRAS_NAME` env var
- **workspace.css** — Artifact browser styles, filter chips, map container

### Lua Scenarios

- **fresh_mock.lua** (new) — Full flow: app_ready → publish identity URI → connect peers via `/tmp/indras-mesh/` → 4-phase artifact seeding (create → share → grant → receive)
- **mock_artifacts.lua** (new) — Same 4-phase seeding without auto-connect (for `--mock`)

### Script & CLI

- **se** — `--remock` flag (clean + auto-connect + seed), `INDRAS_PEERS` env var, `--features lua-scripting`, `/tmp/indras-mesh` cleanup
- **main.rs** — `--seed-script=` flag (runs Lua without exit, unlike `--script=`)

### Deleted

- **mock_artifacts.rs** — Replaced by Lua-scripted seeding

## Merge Steps

```bash
cd /Users/truman/Code/IndrasNetwork

# 1. Merge
git merge feature/artifact-browser

# 2. Verify
cargo build -p indras-artifacts
cargo test -p indras-artifacts
cargo build -p indras-network
cargo test -p indras-network --lib artifact_index
cargo build -p indras-workspace --features lua-scripting

# 3. Clean up worktree
git worktree remove ../IndrasNetwork-artifact-browser

# 4. Delete the branch
git branch -d feature/artifact-browser
```

## If Not Fast-Forward

```bash
# Option A: Merge (preserves history)
git merge feature/artifact-browser

# Option B: Rebase branch first (linear history)
git checkout feature/artifact-browser
git rebase main
git checkout main
git merge feature/artifact-browser
```

Conflicts would likely be in:
- `crates/indras-network/src/artifact_index.rs` — if artifact index was modified on main
- `crates/indras-network/src/home_realm.rs` — if HomeRealm methods were added on main
- `crates/indras-workspace/src/components/app.rs` — if app component was restructured
- `crates/indras-network/src/document.rs` — if document schema changed
- `se` — if launch script was modified

## Test Summary

- indras-artifacts integration tests pass
- indras-network artifact_index tests pass
- indras-workspace builds cleanly with `--features lua-scripting`, zero warnings
- Lua seeding produces 12 artifacts per instance with consistent BLAKE3 ArtifactIds

All pass as of commit `7ab5438`.

---

## Update: Artifact Detail Modal (commit `519dc45`)

### Additional Changes

- **Column rename** — `Nearby/Distant/Untagged` → `Local/Global/Digital` (commit `b49d157`)
- **Artifact detail modal** — Click any artifact card to open a detail overlay showing thumbnail/preview, name, size, MIME type, status, ID, owner, grant count, origin, and distance
- **on_click prop** — `ArtifactGallery` and `ArtifactCard` in `indras-ui` now accept an optional `on_click` callback

### Files Changed

- `crates/indras-ui/src/artifact_display.rs` — `on_click: Option<EventHandler<ArtifactDisplayInfo>>` prop
- `crates/indras-workspace/src/components/artifact_browser.rs` — `ArtifactDetailModal` component, selected-artifact state
- `crates/indras-workspace/assets/workspace.css` — Modal overlay styles (reuses `.pass-story-overlay` pattern)
