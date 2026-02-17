# indras-sync-engine — AI Agent Guide

## Purpose

App layer built on top of `indras-network`. Adds domain-specific functionality (quests, blessings, tokens of gratitude, attention tracking, humanness attestation) via extension traits on `Realm` and `HomeRealm`.

Holds an `Arc<IndrasNetwork>` and uses `impl_document_schema!` to register its CRDT document types.

## Architecture: Extension Trait Pattern

Domain methods are added to `Realm` via traits, not by modifying `indras-network`:

```rust
// In realm_quests.rs
pub trait RealmQuests {
    async fn create_quest(&self, title: &str, desc: &str, ...) -> Result<QuestId>;
    async fn complete_quest(&self, quest_id: &QuestId) -> Result<()>;
}

impl RealmQuests for Realm { ... }
```

Usage:
```rust
use indras_sync_engine::prelude::*;
realm.create_quest("Review doc", "Please review", None, my_id).await?;
```

## Module Map

### Domain Modules (data types + logic)

| Module | Key Types | What It Does |
|--------|-----------|-------------|
| `quest.rs` | `Quest`, `QuestDocument`, `QuestId`, `QuestPriority`, `QuestClaim` | Task/quest lifecycle |
| `note.rs` | `Note`, `NoteDocument`, `NoteId` | Collaborative notes |
| `message.rs` | `StoredMessage`, `MessageDocument`, `MessageId`, `MessageContent` | Persistent message storage |
| `blessing.rs` | `Blessing`, `BlessingDocument`, `BlessingId`, `ClaimId` | Blessings for completed work |
| `attention.rs` | `AttentionDocument`, `QuestAttention`, `AttentionSwitchEvent` | Attention tracking per realm |
| `token_of_gratitude.rs` | `TokenOfGratitude`, `TokenOfGratitudeDocument` | Gratitude tokens |
| `token_valuation.rs` | `SubjectiveTokenValue`, `subjective_value` | Token value with steward chain decay |
| `humanness.rs` | `HumannessAttestation`, `HumannessDocument`, `Delegation` | Humanness attestation chains |
| `sentiment.rs` | `SentimentRelayDocument`, `RelayedSentiment`, `SentimentView` | Relayed sentiment across contacts |
| `proof_folder.rs` | `ProofFolder`, `ProofFolderDocument`, `ProofFolderArtifact` | Proof-of-service folders |
| `story_auth.rs` | `StoryAuth`, `AuthResult` | Story-based authentication |
| `rehearsal.rs` | `RehearsalState` | Story rehearsal state |
| `bioregion_catalog.rs` | - | Bioregional delegation catalog |
| `content.rs` | `SyncContent` | Extended content type for sync engine |

### Extension Traits on Realm

| Module | Trait | Methods |
|--------|-------|---------|
| `realm_quests.rs` | `RealmQuests` | `create_quest`, `complete_quest`, `claim_quest`, ... |
| `realm_notes.rs` | `RealmNotes` | `create_note`, `edit_note`, `list_notes`, ... |
| `realm_messages.rs` | `RealmMessages` | Domain-level message operations |
| `realm_chat.rs` | `RealmChat` | Chat-specific operations |
| `realm_blessings.rs` | `RealmBlessings` | `give_blessing`, `list_blessings`, ... |
| `realm_attention.rs` | `RealmAttention` | `record_attention`, `get_attention`, ... |
| `realm_tokens.rs` | `RealmTokens` | Token of gratitude operations |
| `realm_humanness.rs` | `RealmHumanness` | Humanness attestation operations |
| `realm_proof_folders.rs` | `RealmProofFolders` | Proof folder management |

### Extension Traits on HomeRealm

| Module | Trait |
|--------|-------|
| `home_realm_quests.rs` | `HomeRealmQuests` |
| `home_realm_notes.rs` | `HomeRealmNotes` |

### Core

| Module | Type | What It Does |
|--------|------|-------------|
| `sync_engine.rs` | `SyncEngine` | Holds `Arc<IndrasNetwork>`, entry point for app layer |
| `prelude.rs` | - | Convenience re-exports |

## Adding a New Domain Module

1. Create `my_domain.rs` with data types + `MyDomainDocument` struct
2. Create `realm_my_domain.rs` with `pub trait RealmMyDomain` + `impl RealmMyDomain for Realm`
3. Add `pub mod my_domain;` and `pub mod realm_my_domain;` to `lib.rs`
4. Add `MyDomainDocument` to the `impl_document_schema!` macro call in `lib.rs`
5. Re-export the trait: `pub use realm_my_domain::RealmMyDomain;`
6. Add to `prelude.rs`

## Gotchas

- All document types use `impl_document_schema!` with default merge (LWW replacement)
- Extension traits are on `Realm` from `indras-network`, not on `SyncEngine`
- `SyncEngine::new()` takes `Arc<IndrasNetwork>`, not owned
- The `STEWARD_CHAIN_DECAY` constant controls how token value decays through stewardship transfers

## Dependencies

Internal: `indras-network`, `indras-artifacts`

## Testing

```bash
cargo test -p indras-sync-engine
```
