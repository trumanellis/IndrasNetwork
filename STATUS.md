# indras-vault-sync — Implementation Status

## What's Built

### Core Crate (`crates/indras-vault-sync/`)
- **vault_file.rs** — `VaultFile`, `ConflictRecord` types, 60s conflict window
- **vault_document.rs** — `VaultFileDocument` with set-union CRDT merge, LWW per file, conflict detection, resolution propagation
- **realm_vault.rs** — `RealmVault` extension trait on `Realm`
- **watcher.rs** — `notify`-based FS watcher with debounce + echo suppression
- **sync_to_disk.rs** — Remote CRDT changes → local filesystem writer
- **vault.rs** — `Vault` orchestrator with cached `Document<VaultFileDocument>` handle
- **26 unit tests passing**

### Lua Bindings (`simulation/src/lua/bindings/vault_sync.rs`)
- Full API: `VaultSync.create`, `VaultSync.join`, `:scan`, `:write_file`, `:delete_file`, `:list_files`, `:list_conflicts`, `:resolve_conflict`, `:stop`

### E2E Test Scenarios (5 total)

| Scenario | Status | What it tests |
|----------|--------|---------------|
| `live_vault_sync.lua` | PASS | Basic 7-phase: create/join, write, LWW, conflict detect, delete, convergence |
| `live_vault_scan_convergence.lua` | PASS | Pre-populated scan, hash-level convergence, binary dedup (8 phases) |
| `live_vault_offline_rejoin.lua` | PASS | Offline/rejoin via relay, node restart with persistence (9 phases) |
| `live_vault_conflict_lifecycle.lua` | PASS | Full conflict lifecycle: detect, resolve, sync resolution, same-content-no-conflict (8 phases) |
| `live_vault_stress.lua` | PARTIAL | Phases 1-2 pass (50 files sync to 3 nodes). Phase 3 (rapid edits convergence) hangs — needs longer timeouts or debouncing |

## Bugs Fixed During Testing

1. **Delta sync causes stale reads** — `extract_delta` sent compact deltas that `Document::load_or_create` couldn't find when creating fresh handles. Fixed by disabling delta (always sends full state — vault index is small metadata).

2. **Cached Document handle** — Each `vault_index()` call was creating a new `Document<T>` that loaded potentially stale state from event history. Fixed by caching the Document handle in the `Vault` struct so all operations share the same in-memory state.

3. **Conflict resolution not propagating** — `merge()` skipped remote conflicts with matching `(path, loser_hash)` even when the remote version was resolved. Fixed by propagating the `resolved` flag during merge.

## Remaining Work

### Stress Test Fix (`live_vault_stress.lua`)
- Phase 3 (rapid sequential edits) hangs waiting for convergence
- Root cause: 10 rapid edits flood the sync with full-state messages; remote nodes may not converge within the 15s timeout
- Fix options:
  - Increase timeout to 30s for rapid-edit convergence checks
  - Add debounce in the test (longer sleep between edits)
  - Or accept that rapid edits are eventually consistent and test final state only

### Not Yet Committed
All changes are in the jj working copy. Need to `jj describe` + `jj bookmark set main` + `jj git push` when ready.

## Files Modified Since Last Push

```
M Cargo.lock
M Cargo.toml (workspace members + deps)
M crates/indras-vault-sync/src/vault.rs (cached Document handle)
M crates/indras-vault-sync/src/vault_document.rs (disable delta, fix conflict merge)
M simulation/Cargo.toml (add vault-sync dep)
M simulation/src/lua/bindings/live_network.rs (pub(crate) for vault_sync access)
M simulation/src/lua/bindings/mod.rs (add vault_sync module)
M simulation/src/lua/mod.rs (register vault_sync bindings)
A crates/indras-vault-sync/* (entire new crate)
A simulation/src/lua/bindings/vault_sync.rs
A simulation/scripts/scenarios/live_vault_sync.lua
A simulation/scripts/scenarios/live_vault_scan_convergence.lua
A simulation/scripts/scenarios/live_vault_offline_rejoin.lua
A simulation/scripts/scenarios/live_vault_conflict_lifecycle.lua
A simulation/scripts/scenarios/live_vault_stress.lua
```
