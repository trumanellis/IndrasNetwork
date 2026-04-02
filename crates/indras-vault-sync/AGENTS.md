# indras-vault-sync

P2P Obsidian vault synchronization over Indra's Network.

## Purpose

Syncs a local directory (vault) across peers using a CRDT document to track
file metadata. Each file is content-addressed via BLAKE3 and stored in a
`BlobStore`. Concurrent edits within a 60-second window produce conflict
copies rather than silently overwriting.

## Architecture

```
Local FS  ──▶  VaultWatcher  ──▶  VaultFileDocument (CRDT)  ──▶  Network peers
                                         │
Network peers  ──▶  VaultFileDocument  ──▶  SyncToDisk  ──▶  Local FS
```

## Module Map

| Module | Purpose |
|---|---|
| `vault_file` | Core types: `VaultFile`, `ConflictRecord` |
| `vault_document` | `VaultFileDocument` — CRDT schema with LWW + conflict detection |
| `realm_vault` | `RealmVault` extension trait on `Realm` |
| `watcher` | FS watcher: local changes -> vault-index document |
| `sync_to_disk` | Document subscriber: remote changes -> local FS |
| `relay_sync` | Relay-backed blob replication: push/pull file content via relay |
| `vault` | `Vault` orchestrator tying everything together |

## Key Design Decisions

- **LWW per file path**: each file has a `modified_ms` timestamp; most-recent wins.
- **Conflict window**: edits <60s apart with different hashes create `ConflictRecord`
  entries and write `.conflict-<hash>` copies to disk.
- **Tombstones**: deleted files are marked `deleted: true` rather than removed,
  so deletes propagate via merge.
- **BlobStore**: file content is stored content-addressed under `.indras/blobs/`
  inside the vault directory.
- **Watcher suppression**: when sync-to-disk writes a file, the watcher is
  temporarily suppressed for that path to avoid echo loops.
- **Relay blob sync**: file content is pushed to the relay (Connections tier)
  after local storage, and pulled from the relay when `SyncToDisk` can't find
  a blob locally. Each vault gets a deterministic `vault-blobs` InterfaceId.
  Blobs >900KB are skipped (relay wire limit is 1MB).

## Dependencies

- `indras-network`: Realm, Document, DocumentSchema, MemberId
- `indras-storage`: BlobStore, ContentRef
- `notify`: cross-platform FS watching
- `blake3`: content hashing
- `dashmap`: concurrent suppression map
