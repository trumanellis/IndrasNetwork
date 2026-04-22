# indras-sync-engine — AI Agent Guide

## Purpose

App layer built on top of `indras-network`. Adds domain-specific functionality (intentions, blessings, tokens of gratitude, attention tracking, humanness attestation) via extension traits on `Realm` and `HomeRealm`.

Holds an `Arc<IndrasNetwork>` and registers its CRDT document types with custom merge semantics.

## Architecture: Extension Trait Pattern

Domain methods are added to `Realm` via traits, not by modifying `indras-network`:

```rust
// In realm_intentions.rs
pub trait RealmIntentions {
    async fn create_intention(&self, title: &str, desc: &str, ...) -> Result<IntentionId>;
    async fn complete_intention(&self, id: IntentionId, caller: MemberId) -> Result<()>;
}

impl RealmIntentions for Realm { ... }
```

Usage:
```rust
use indras_sync_engine::prelude::*;
realm.create_intention("Review doc", "Please review", None).await?;
```

## Module Map

### Domain Modules (data types + logic)

| Module | Key Types | What It Does |
|--------|-----------|-------------|
| `intention.rs` | `Intention`, `IntentionDocument`, `IntentionId`, `IntentionKind`, `IntentionPriority`, `ServiceClaim` | Intention lifecycle with Quest/Need/Offering/Intention subtypes |
| `note.rs` | `Note`, `NoteDocument`, `NoteId` | Collaborative notes with tombstone deletion |
| `blessing.rs` | `Blessing`, `BlessingDocument`, `BlessingId`, `ClaimId` | Blessings for completed work |
| `attention.rs` | `AttentionDocument`, `IntentionAttention`, `AttentionSwitchEvent` | Attention tracking per realm |
| `token_of_gratitude.rs` | `TokenOfGratitude`, `TokenOfGratitudeDocument` | Gratitude tokens with stewardship chains |
| `token_valuation.rs` | `SubjectiveTokenValue`, `subjective_value` | Token value with steward chain decay |
| `humanness.rs` | `HumannessAttestation`, `HumannessDocument`, `Delegation`, `BioregionalLevel`, `DelegationError` | Humanness attestation chains |
| `sentiment.rs` | `SentimentRelayDocument`, `RelayedSentiment`, `SentimentView`, `DEFAULT_RELAY_ATTENUATION` | Relayed sentiment across contacts |
| `proof_folder.rs` | `ProofFolder`, `ProofFolderDocument`, `ProofFolderArtifact`, `ProofFolderError`, `ProofFolderId` | Proof-of-service folders |
| `story_auth.rs` | `StoryAuth`, `AuthResult` | Story-based authentication |
| `steward_recovery.rs` | `StewardId`, `StewardManifest`, `StewardAssignment`, `PreparedRecovery`, `prepare_recovery`, `recover_encryption_subkey`, `save_manifest`, `load_manifest` | Shamir K-of-N steward recovery for the encryption subkey; offline orchestration + JSON manifest |
| `steward_enrollment.rs` | `StewardInvitation`, `StewardResponse`, `EnrollmentStatus`, `invite_doc_key`, `response_doc_key` | Plan-A handshake: invitation + acceptance CRDT docs over DM realms |
| `share_delivery.rs` | `ShareDelivery`, `HeldBackup`, `StewardHoldings`, `share_delivery_doc_key` | Plan-A encrypted-share delivery doc + steward-side holdings cache |
| `recovery_protocol.rs` | `RecoveryRequest`, `ShareRelease`, `recovery_request_doc_key`, `share_release_doc_key` | Plan-A recovery-request + release CRDT docs |
| `device_roster.rs` | `DeviceRoster`, `DEVICE_ROSTER_DOC_KEY` | Plan-B per-account device-cert roster (home-realm doc) |
| `account_root_cache.rs` | `save_pending_root`, `load_pending_root`, `clear_pending_root` | Plan-B temporary root-sk stash between creation and first split |
| `account_root_envelope.rs` | `AccountRootEnvelope`, `seal_account_root`, `unseal_account_root` | Plan-B ChaCha20-Poly1305 envelope for the root sk under a Shamir-split wrapping key |
| `peer_verification.rs` | `verify_peer_device`, `load_device_roster` | Plan-B peer-admission helpers gating on the roster |
| `backup_peers.rs` | `BackupPeerAssignment`, `BackupPeerPlan`, `backup_role_doc_key`, `DEFAULT_BACKUP_RESPONSIBILITY` | Plan-C Backup-Peer role CRDT + top-N selection ranking |
| `file_shard.rs` | `FileShard`, `PreparedShardSet`, `prepare_file_shards`, `reconstruct_file`, `file_shard_doc_key` | Plan-C erasure-coded file-shard pipeline with per-file / account-wrapping double encryption |
| `rehearsal.rs` | `RehearsalState` | Story rehearsal state |
| `bioregion_catalog.rs` | - | Bioregional delegation catalog |
| `content.rs` | `SyncContent` | Extended content type for sync engine |

### Extension Traits on Realm

| Module | Trait | Methods |
|--------|-------|---------|
| `realm_intentions.rs` | `RealmIntentions` | `create_intention`, `complete_intention`, `submit_service_claim`, `verify_service_claim`, ... |
| `realm_notes.rs` | `RealmNotes` | `create_note`, `edit_note`, `list_notes`, ... |
| `realm_chat.rs` | `RealmChat` | Chat operations (sole chat interface) |
| `realm_blessings.rs` | `RealmBlessings` | `bless_claim`, `list_blessings`, ... |
| `realm_attention.rs` | `RealmAttention` | `focus_on_intention`, `intention_attention`, ... |
| `realm_tokens.rs` | `RealmTokens` | Token pledge/release/withdraw with authorization |
| `realm_humanness.rs` | `RealmHumanness` | Humanness attestation operations |
| `realm_proof_folders.rs` | `RealmProofFolders` | Proof folder management |

### Extension Traits on HomeRealm

| Module | Trait |
|--------|-------|
| `home_realm_intentions.rs` | `HomeRealmIntentions` |
| `home_realm_notes.rs` | `HomeRealmNotes` |

### Core

| Module | Type | What It Does |
|--------|------|-------------|
| `sync_engine.rs` | `SyncEngine` | Holds `Arc<IndrasNetwork>`, entry point for app layer |
| `prelude.rs` | - | Convenience re-exports |

## CRDT Merge Semantics

Three merge strategies are used:

| Strategy | Documents | How It Works |
|----------|-----------|-------------|
| **Set-union by ID** | `IntentionDocument`, `NoteDocument`, `ProofFolderDocument` | Manual `DocumentSchema` impl. Deduplicates by unique ID. `NoteDocument` also does last-writer-wins per-ID for updates/tombstones. |
| **Event-log append** | `BlessingDocument`, `AttentionDocument`, `TokenOfGratitudeDocument` | Default `impl_document_schema!`. Events are append-only, deduped by event ID. |
| **LWW replacement** | `HumannessDocument`, `SentimentRelayDocument` | Default `impl_document_schema!`. Last writer wins for the whole document. |

## Authorization

Critical operations verify the caller's role before proceeding:

- `complete_intention()` / `verify_service_claim()` — caller must be intention creator
- `pledge_token()` / `release_token()` / `withdraw_token()` — caller must be current token steward
- `bless_claim()` — caller must have attention events for the intention

## Adding a New Domain Module

1. Create `my_domain.rs` with data types + `MyDomainDocument` struct
2. Create `realm_my_domain.rs` with `pub trait RealmMyDomain` + `impl RealmMyDomain for Realm`
3. Add `pub mod my_domain;` and `pub mod realm_my_domain;` to `lib.rs`
4. Add `MyDomainDocument` to `impl_document_schema!` (for LWW) or implement `DocumentSchema` manually (for set-union)
5. Re-export the trait: `pub use realm_my_domain::RealmMyDomain;`
6. Add to `prelude.rs`

## Gotchas

- `IntentionDocument`, `NoteDocument`, `ProofFolderDocument` have **manual** `DocumentSchema` impls with set-union merge — they are NOT in the `impl_document_schema!` macro
- `NoteDocument` uses tombstone deletion (`note.deleted = true`) to survive CRDT merge
- Extension traits are on `Realm` from `indras-network`, not on `SyncEngine`
- `SyncEngine::new()` takes `Arc<IndrasNetwork>`, not owned
- `IntentionKind` enum: `Quest`, `Need`, `Offering`, `Intention` (default)
- The old `RealmMessages` system was removed — only `RealmChat` remains

## Dependencies

Internal: `indras-network`, `indras-artifacts`

## Testing

```bash
cargo test -p indras-sync-engine
```
