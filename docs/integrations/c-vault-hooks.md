# C.4 / C.5 vault-integration guide

Plan C's primitives (`erasure`, `file_shard`, `backup_peers`) are
shipped and tested, but they're not yet wired to the file system
or the UI. This note spells out exactly what hooks are missing and
what they need to call. Intended for whichever agent eventually
owns the vault-manager changes (currently reserved for agent1's
braid-sync work).

## What's already there

- `indras_crypto::erasure::{encode, decode}` — Reed-Solomon, GF(2^8).
- `indras_sync_engine::file_shard`:
  - `prepare_file_shards(bytes, file_id, label, wrapping_key, K, N, ts)`
  - `reconstruct_file(shards, wrapping_key)`
  - `file_shard_doc_key(file_id, shard_index)` → `_file_shard:{hex}:{i}`
- `indras_sync_engine::backup_peers::BackupPeerAssignment` CRDT +
  `select_top` ranking.
- Plan-B plumbing means the wrapping key `W` is already available
  on a recovered device (unseal via `_account_root_envelope`). On
  a signed-in device `W` lives only in memory during the session
  that just finished steward split — so it either needs to be
  re-split + re-fetched from stewards on every file-save (hostile
  to UX) **or** cached alongside the existing
  `account_root_cache` (and torn down on logout).

Open design choice: see "Where does W live locally?" below.

## C.4 — publish-on-save hook

### Where to hook

The vault watcher is notify-based and lives under
`crates/synchronicity-engine/src/vault_manager.rs` (+ friends).
Plan C needs a shard-on-write callback — *not* the GC loop, which
is agent1's territory per CLAUDE.md.

Minimum viable wiring:

1. Detect "file N was written and has settled" (e.g. after a
   debounce window — reuse the same debounce the CRDT file-sync
   already applies).
2. For each Backup Peer in the account's roster (see
   `backup_peers::list_active_assignments` — not yet written; the
   selection helper exists, the bridge-level scan doesn't):
   a. Open the sender↔peer DM realm.
   b. Publish one `_file_shard:{file_id_hex}:{i}` doc with the
      corresponding shard.

### API sketch

```rust
// crates/synchronicity-engine/src/recovery_bridge.rs
pub async fn publish_file_shards(
    network: Arc<IndrasNetwork>,
    wrapping_key: &[u8; 32],
    backup_peers: Vec<BackupPeerUid>,  // hex UIDs; K = len - parity
    file_bytes: &[u8],
    file_label: String,
) -> Result<usize, String>
```

The bridge fn owns: compute `file_id = blake3(file_bytes)`, call
`prepare_file_shards`, for each (peer, shard) pair open the DM
realm + publish. Failures per peer are swallowed; caller sees
delivered count.

### Pre-reqs before enabling

- Make sure `backup_peers` are real — C.2's CRDT is defined but
  the invitation-flow UI doesn't exist yet. A status-bar
  "Backup peers: 0 of 5" badge with a picker overlay mirrors the
  existing Steward invitation flow.
- Surface the per-file `K, N` parameters in settings. Default
  `K = 3, N = 5` is reasonable; "backup resilience" slider in
  settings can tune it.

## C.5 — recovery-time re-pull

### Entry point

`recovery_bridge::assemble_and_authenticate` already signs the
new device's cert + zeros the root. The follow-up call (in the
same async task) should:

1. Scan every DM realm for `_file_shard:*` docs the recovering
   user published (docs keyed by their own `file_id_hex`, written
   by themselves).
2. Group shards by `file_id_hex`, pull from multiple DM realms
   until each group has ≥ `data_threshold` survivors.
3. For each complete file: `reconstruct_file` + write to the
   vault directory.
4. Emit UI progress events so the Recovery overlay can render
   "12 / 47 files restored".

### API sketch

```rust
pub async fn repull_all_backups(
    network: Arc<IndrasNetwork>,
    wrapping_key: &[u8; 32],
    vault_path: &Path,
    on_progress: impl Fn(FileRecoveryProgress),
) -> Result<FileRecoverySummary, String>
```

### Subtlety: W availability

On recovery, the new device has `W` in memory right after the
steward flow completes. If the app is restarted before re-pull
finishes, `W` vanishes. Mitigations:

- **Do the re-pull in the same session as recovery** (simple, UX
  constraint).
- **Cache W to disk** behind a hardware-key wrap (Phase 3 work).
- **Don't cache W — re-split on every save.** Rejected: heavy
  gossip traffic, bad UX.

Recommend the first for the initial cut.

## Where does W live locally?

This is the most important design decision the vault-integration
slice has to answer.

Options:

1. **In-session only** (easy, safest). `W` is populated at steward
   split time (enrollment) and at recovery (assemble). It lives in
   a `Arc<Mutex<Option<[u8; 32]>>>` on `AppState`. Every file save
   needs the app to have been logged in recently. On restart W is
   gone until next recovery.
2. **Disk-cached like `account_root.pending`** (convenient,
   leakier). Works offline but widens the attacker's window. See
   `account_root_cache.rs` for the pattern.
3. **Hardware-key wrapped** (Phase 3). Out of scope here.

Recommend option 1 for C.4+C.5 MVP. The limitation is "you can
only save backup shards while signed in" — which in practice
means after account creation / steward split / recovery, the
session already has W, and file writes from that session will
shard to peers. App restarts without re-authentication skip the
shard step but don't lose local files.

## Wiring tips

- Don't touch `vault/sync_all.rs`. Put new save-hook logic in a
  sibling file (`vault/backup_publish.rs`?) and call it from
  wherever the vault watcher fires today.
- Don't touch the GC loop. Shards are independent docs; letting
  the GC handle them is a separate hardening pass.
- Reuse the existing ipc message plumbing for progress events
  rather than adding new channels — C.5's progress UI is a new
  consumer of the same pipe that sync_panel uses today.

## Testing

- Unit level: the primitives already cover crypto + erasure.
- Crate level: add `tests/data_backup_wiring.rs` that drives the
  publish-on-save hook over a temporary vault + a stubbed peer
  list; assert `_file_shard:*` docs land in expected realms.
- Multi-peer iroh E2E: still blocked on the `DirectConnect`
  harness missing from `indras-network/tests/`.

## Related docs

- `articles/recovery-architecture.md` — full A+B+C overview.
- `plans/frictionless-recovery.md` — slice-level plan.
- `docs/migrations/account-root.md` — legacy-account compat.
