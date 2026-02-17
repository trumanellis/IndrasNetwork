# indras-network — AI Agent Guide

## Purpose

Single import surface for building P2P apps on Indra's Network. Wraps all lower-level crates (`indras-node`, `indras-sync`, `indras-transport`, `indras-storage`, `indras-crypto`, `indras-messaging`, `indras-artifacts`) into a high-level SDK.

Re-exports the entire `indras-artifacts` crate so consumers never need to depend on lower-level crates directly.

## Module Map

| Module | Key Types | What It Does |
|--------|-----------|-------------|
| `network.rs` | `IndrasNetwork`, `GlobalEvent`, `IdentityBackup` | Main entry point, lifecycle, realm management |
| `realm.rs` | `Realm`, `RealmId` | Collaborative space: messaging, documents, artifacts |
| `config.rs` | `NetworkConfig`, `NetworkBuilder`, `Preset` | Builder pattern configuration |
| `document.rs` | `Document<T>`, `DocumentSchema`, `DocumentChange` | Typed CRDT documents with auto-sync |
| `home_realm.rs` | `HomeRealm`, `HomeArtifactMetadata` | Personal artifact storage per identity |
| `contacts.rs` | `ContactsRealm`, `ContactEntry`, `ContactsDocument` | Contact management with sentiment |
| `message.rs` | `Message`, `Content`, `MessageId` | Messaging with 13 content variants |
| `member.rs` | `Member`, `MemberId`, `MemberEvent`, `MemberInfo` | Peer identity and presence |
| `artifact.rs` | `ArtifactDownload`, `DownloadProgress` | Artifact download with progress |
| `artifact_index.rs` | `ArtifactIndex`, `HomeArtifactEntry` | CRDT artifact tree with access control |
| `artifact_sync.rs` | `ArtifactSyncRegistry` | Per-artifact gossip sync management |
| `chat_message.rs` | `RealmChatDocument`, `EditableChatMessage` | Editable versioned chat messages |
| `access.rs` | `GrantError`, `RevokeError`, `TransferError`, `TreeError` | Network-layer access control errors |
| `direct_connect.rs` | `ConnectionNotify`, `KeyExchangeRegistry` | Identity-is-connection pattern |
| `encounter.rs` | `EncounterHandle`, `EncounterExchangePayload` | 6-digit spoken codes for in-person discovery |
| `identity_code.rs` | `IdentityCode` | bech32m identity encoding (`indra1...`) |
| `invite.rs` | `InviteCode` | Realm invite URIs (`indra:realm:...`) |
| `encryption.rs` | `ArtifactKey`, `EncryptedArtifactKey` | Per-artifact encryption |
| `read_tracker.rs` | `ReadTrackerDocument` | Per-member LWW read positions |
| `realm_alias.rs` | `RealmAliasDocument` | Custom realm nicknames |
| `world_view.rs` | `WorldView` | Debug snapshot of network state |
| `escape.rs` | Re-exports | Escape hatch to lower-level types |
| `error.rs` | `IndraError`, `Result` | Error types |

## Lifecycle

```
IndrasNetwork::new(path)  →  network.start()  →  use realms/docs/artifacts  →  network.stop()
```

- `start()` initializes transport, joins inbox realm, begins listening
- Many methods (`create_realm`, `join`, `connect`, `home_realm`) call `start()` implicitly if not yet running
- `stop()` tears down all interfaces, cancels syncs, closes connections

## Common Tasks

**Adding a content type**: Add variant to `Content` enum in `message.rs`, add constructor method, update `From` impls.

**Adding a realm method**: Add to `impl Realm` in `realm.rs`. Access node via `self.node`. For domain methods, prefer extension traits in `indras-sync-engine`.

**Adding a document type**: Define struct with `Default + Clone + Serialize + Deserialize`, call `impl_document_schema!` macro, access via `realm.document::<MyType>("name")`.

## Key Patterns

- **BLAKE3 deterministic IDs**: All realm/interface IDs derived via BLAKE3 with domain prefixes (`"home-realm-v1:"`, `"inbox-v1:"`, `"artifact-sync-v1:"`, `"encounter-v1:"`, etc.)
- **Gossip-per-artifact**: Each artifact with active grantees gets its own gossip interface
- **DocumentSchema merge**: Default is LWW replacement; `RealmChatDocument` uses set-union by message ID
- **DM realm symmetry**: `dm_story_id(A, B) == dm_story_id(B, A)` — both peers agree on the same ID

## Gotchas

- `realm()` (peer-based) requires `join_contacts_realm()` to have been called first
- `block_contact()` also requires contacts realm
- Document names starting with `_` are treated as internal (skip registry)
- `DocumentSchema::merge()` defaults to full replacement — override for set-union semantics
- Never cache Automerge `ObjId`s — they go stale after sync/merge
- The `members()` method is deprecated — use `member_events()` instead

## Dependencies

Internal: `indras-node`, `indras-core`, `indras-messaging`, `indras-sync`, `indras-storage`, `indras-transport`, `indras-crypto`, `indras-artifacts`

## Testing

```bash
cargo test -p indras-network
```
