# indras-artifacts — AI Agent Guide

## Purpose

Domain types for the artifact/attention economy. Defines the data model that all higher-level crates build on: artifacts, access control, attention tracking, peering, and token valuation. No networking — pure data structures and traits.

## Key Concepts

### Artifact Hierarchy

```
Vault (top-level container, one per user)
├── Story (narrative thread)
│   ├── LeafArtifact (Message, Image, File, Token, Attestation)
│   └── Gallery (collection of images/files)
├── Exchange (trade/gift between peers)
├── Request (ask for artifacts or actions)
├── Quest / Need / Offering / Intention
└── Collection / Document / Inbox
```

### ArtifactId

```rust
enum ArtifactId {
    Blob([u8; 32]),  // Leaf — content-addressed by BLAKE3 hash of payload
    Doc([u8; 32]),   // Tree — random or deterministic unique ID
}
```

### Tree Composition

Artifacts form parent-child trees. Children inherit access from parents. `TreeArtifact` holds `references: Vec<ArtifactRef>` for ordered child references with labels.

## Module Map

| Module | Key Types | What It Does |
|--------|-----------|-------------|
| `artifact.rs` | `Artifact`, `LeafArtifact`, `TreeArtifact`, `ArtifactId`, `ArtifactRef`, `LeafType`, `TreeType`, `PlayerId` | Core artifact types and ID generation |
| `access.rs` | `AccessGrant`, `AccessMode`, `ArtifactProvenance`, `ArtifactStatus`, `ProvenanceType` | Access control and lifecycle |
| `attention.rs` | `AttentionLog`, `AttentionSwitchEvent`, `AttentionValue`, `DwellWindow`, `compute_heat`, `extract_dwell_windows` | Attention tracking, heat computation, and dwell window extraction |
| `token.rs` | `compute_token_value` | Token value derivation from attention data |
| `vault.rs` | `Vault` | Personal vault (top-level container) |
| `story.rs` | `Story` | Narrative thread of artifacts |
| `exchange.rs` | `Exchange` | Trade/gift between peers |
| `request.rs` | `Request` | Request for artifacts or actions |
| `intention.rs` | `Intention` | Goal with proofs, attention tokens, and pledges |
| `peering.rs` | `PeerEntry`, `PeerRegistry`, `MutualPeering` | Peer relationship tracking |
| `store.rs` | `ArtifactStore`, `PayloadStore`, `AttentionStore` + in-memory impls | Storage traits |
| `error.rs` | `VaultError` | Error types |

## Access Control

### AccessMode

| Variant | Behavior |
|---------|----------|
| `Revocable` | Default. Owner can revoke at any time |
| `Permanent` | Cannot be revoked once granted |
| `Timed { expires_at: i64 }` | Auto-expires at Unix timestamp |
| `Transfer` | Ownership transfer — grantee becomes new steward |

### ArtifactStatus

`Active` → `Recalled { recalled_at }` or `Transferred { to, transferred_at }`

### AccessGrant

```rust
struct AccessGrant { grantee: PlayerId, mode: AccessMode, granted_at: i64, granted_by: PlayerId }
```

## Attention / Heat System

1. `AttentionLog` records `AttentionSwitchEvent`s (focus changes between artifacts)
2. `compute_heat(log)` → `AttentionValue` (how "hot" an artifact is based on recent attention)
3. `compute_token_value(attention_data)` → derives economic value from attention

## Store Traits

| Trait | Methods | Purpose |
|-------|---------|---------|
| `ArtifactStore` | `store`, `get`, `list`, `delete` | Artifact metadata persistence |
| `PayloadStore` | `store_payload`, `get_payload`, `delete_payload` | Binary content persistence |
| `AttentionStore` | `record_event`, `get_log`, `compute_heat` | Attention data persistence |

Each has an `InMemory*` implementation for testing.

## ID Generation

- `leaf_id(payload: &[u8])` → `ArtifactId::Blob(blake3::hash(payload))`
- `generate_tree_id()` → `ArtifactId::Doc(random::<[u8; 32]>())`
- `dm_story_id(a, b)` → deterministic, symmetric (`dm_story_id(A, B) == dm_story_id(B, A)`)

## Gotchas

- `PlayerId` is `[u8; 32]`, aliased from iroh `PublicKey` bytes
- `ArtifactId` is `Copy` — no clone needed
- `AccessMode::is_expired(now)` only applies to `Timed` variant
- `dm_story_id` sorts the two player IDs before hashing to ensure symmetry

## Dependencies

External only: `serde`, `blake3`, `rand`

## Testing

```bash
cargo test -p indras-artifacts
```
