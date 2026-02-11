# Implementation Plan: `indras-artifacts` Crate

## Context

IndrasNetwork has a new, radically simpler vision built on THREE primitives:

1. **Artifact** — the only data structure (Leaf = immutable blob, Tree = mutable CRDT doc)
2. **Attention Switch Event** — the only event (per-player append-only log)
3. **Mutual Peering** — the only relationship (bidirectional attention log sharing)

Everything else — value, exchange, conversation, composition, discovery — emerges from these three.

We're building a **new standalone crate** (`indras-artifacts`) alongside the existing system. The existing crates remain untouched. This crate implements the pure domain model, designed to be consumed by a future Dioxus spatial browser UI (see UI_ARCHITECTURE.md, DIOXUS_IMPLEMENTATION.md).

**Key insight from UI spec**: The fractal artifact tree IS the UI. Navigation IS attention tracking. The Vault IS an artifact. This crate must model these truths at the data layer.

---

## New Crate: `crates/indras-artifacts`

**Dependencies:** `serde`, `postcard`, `blake3`, `bytes`, `rand`, `chrono`, `thiserror`

Fully self-contained. No dependency on any existing `indras-*` crate. Defines its own `PlayerId` type alias compatible with `MemberId`.

**Persistence-ready**: All types derive Serialize/Deserialize. Storage is accessed via traits with an in-memory implementation for testing. The trait interfaces are designed to map cleanly onto `indras-storage` (BlobStore, EventLog, RedbStorage) or iroh primitives (Blobs, Docs) when integrated later.

---

## Core Types

### ArtifactId (enum, not flat bytes)

```rust
/// Identifies an artifact. The variant tells you how to resolve it.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactId {
    /// Leaf artifact — content-addressed by BLAKE3 hash of payload.
    Blob([u8; 32]),
    /// Tree artifact — random unique ID (like an iroh Doc ID).
    Doc([u8; 32]),
}
```

This distinction is fundamental: resolving a Blob means fetching content by hash, resolving a Doc means opening a mutable CRDT document. The UI layer needs this to know whether to fetch lazily (Blob) or sync eagerly (Doc).

### PlayerId

```rust
pub type PlayerId = [u8; 32];  // == iroh PublicKey bytes. Compatible with existing MemberId.
```

### Artifact (the only data structure)

```rust
pub enum Artifact {
    Leaf(LeafArtifact),
    Tree(TreeArtifact),
}

/// Immutable payload. Content-addressed by BLAKE3 hash.
/// Payload is NOT stored inline — only the hash. Content is fetched lazily
/// when a player attends to the artifact (zooms into it in the UI).
pub struct LeafArtifact {
    pub id: ArtifactId,              // ArtifactId::Blob(blake3_hash)
    pub size: u64,                   // Payload size in bytes (for display without fetching)
    pub steward: PlayerId,           // Transferable authority
    pub audience: Vec<PlayerId>,     // Who can sync/attend
    pub artifact_type: LeafType,
    pub created_at: i64,             // millis since epoch
}

/// Mutable CRDT organizing references to other Artifacts.
/// Syncs eagerly across audience (the tree spine).
pub struct TreeArtifact {
    pub id: ArtifactId,              // ArtifactId::Doc(random_id)
    pub steward: PlayerId,
    pub audience: Vec<PlayerId>,
    pub references: Vec<ArtifactRef>,
    pub metadata: BTreeMap<String, Vec<u8>>,
    pub artifact_type: TreeType,
    pub created_at: i64,
}
```

### Leaf Types

```rust
pub enum LeafType {
    Message,        // Chat message (text payload)
    Image,          // Image file
    File,           // Generic file
    Attestation,    // Humanness attestation
    Token,          // Token of gratitude
    Custom(String),
}
```

### Tree Types

```rust
pub enum TreeType {
    /// The player's personal root space. Every player has exactly one.
    /// Contains refs to all artifacts they steward or have been shared.
    Vault,
    /// Sequential journey through artifacts. The general case.
    /// A conversation is a Story where most leaves are messages.
    Story,
    /// Spatial layout of artifacts (images, etc).
    Gallery,
    /// Ordered text/media collection (vertical reading flow).
    Document,
    /// A request with orbiting offers. Central artifact + tagged offers.
    Request,
    /// Negotiation space between two stewards.
    /// Contains a Story (the negotiation conversation) + refs to the two artifacts being exchanged.
    Exchange,
    /// Generic collection.
    Collection,
    /// Extension point.
    Custom(String),
}
```

### ArtifactRef

```rust
/// A reference from a Tree to a child artifact.
pub struct ArtifactRef {
    pub artifact_id: ArtifactId,
    pub position: u64,              // Ordering key within the tree
    pub label: Option<String>,      // Optional role/label (e.g., "offer", "request-center")
}
```

### Attention Switch (the only event)

```rust
pub struct AttentionSwitchEvent {
    pub player: PlayerId,
    pub from: Option<ArtifactId>,   // None if session start
    pub to: Option<ArtifactId>,     // None if session end / cleared
    pub timestamp: i64,             // millis since epoch
}
```

### Attention Log & Derived Values

```rust
/// Per-player append-only log. Shared read-only with mutual peers.
pub struct AttentionLog {
    pub player: PlayerId,
    pub events: Vec<AttentionSwitchEvent>,
}

/// Derived attention value for an artifact, from one player's perspective.
/// Computed locally from merged peer logs, not stored.
pub struct AttentionValue {
    pub artifact_id: ArtifactId,
    pub unique_peers: usize,        // Count of mutual peers who attended
    pub total_dwell_millis: u64,    // Aggregate dwell time from all peers
    pub heat: f32,                  // Normalized 0.0 (cold) to 1.0 (hot) — UI-ready
}
```

`heat` is a **normalized 0.0–1.0 float** suitable for direct use as a CSS custom property (`--heat`). Computed from recency-weighted attention density across mutual peers in the audience.

### Mutual Peering (the only relationship)

```rust
pub struct PeerEntry {
    pub peer_id: PlayerId,
    pub since: i64,
    pub display_name: Option<String>,
}

pub struct PeerRegistry {
    pub player: PlayerId,
    pub peers: Vec<PeerEntry>,
}

/// Canonical representation: peer_a < peer_b (sorted).
pub struct MutualPeering {
    pub peer_a: PlayerId,
    pub peer_b: PlayerId,
    pub since: i64,
}
```

---

## Concept Mapping (Old → New)

For context on how existing types map to the new model:

| Old Concept | New Concept | How |
|-------------|-------------|-----|
| Realm | Audience (per-artifact) | A realm was a container with members. Audience is a property on each artifact. |
| Document (Automerge) | Tree Artifact | Same CRDT idea, but every doc is an artifact with steward + audience. |
| Artifact (static file) | Leaf Artifact | Content-addressed blobs, same as before. |
| Quest | Request (Tree Artifact) | A request with orbiting offer refs. |
| QuestClaim | Offer (Leaf tagged onto Request) | Proof artifact composed into Request tree. |
| Blessing | Stewardship Transfer | Completing a request = transferring stewardship of a Token artifact. |
| Token of Gratitude | Leaf Artifact with attention history | Value computed from attention, steward chain = transfer history. |
| Attention (quest-scoped) | Attention Switch (artifact-scoped) | Broadened: tracks navigation through entire tree, not just quests. |
| Sentiment/Contacts | Mutual Peering | Simplified: peers are mutual or not. |
| Member / MemberId | Player / PlayerId | Same type, new name. |
| HomeRealm | Vault (TreeType::Vault) | Player's root Tree Artifact. |
| DM Realm | Story with deterministic ID | `blake3("dm-v1:" + sorted(a, b))` → Doc ID for a shared Story. |
| Inbox | Personal Tree Artifact | Connection requests appear as child refs. |

---

## Storage Traits (Persistence-Ready)

The Vault accesses storage through traits. An in-memory implementation is provided. These traits are designed to map cleanly onto existing infrastructure when integrated.

```rust
/// Store and retrieve artifact metadata (steward, audience, type, refs).
/// Maps to: RedbStorage tables, or iroh Doc entries.
pub trait ArtifactStore {
    fn put_artifact(&mut self, artifact: &Artifact) -> Result<()>;
    fn get_artifact(&self, id: &ArtifactId) -> Result<Option<Artifact>>;
    fn list_by_type(&self, tree_type: &TreeType) -> Result<Vec<ArtifactId>>;
    fn list_by_steward(&self, steward: &PlayerId) -> Result<Vec<ArtifactId>>;
    fn update_steward(&mut self, id: &ArtifactId, new_steward: PlayerId) -> Result<()>;
    fn update_audience(&mut self, id: &ArtifactId, audience: Vec<PlayerId>) -> Result<()>;
    fn add_ref(&mut self, tree_id: &ArtifactId, child_ref: ArtifactRef) -> Result<()>;
    fn remove_ref(&mut self, tree_id: &ArtifactId, child_id: &ArtifactId) -> Result<()>;
}

/// Store and retrieve blob payloads (Leaf content).
/// Maps to: BlobStore (content-addressed by BLAKE3), or iroh Blobs.
pub trait PayloadStore {
    fn store_payload(&mut self, payload: &[u8]) -> Result<ArtifactId>;  // returns Blob(hash)
    fn get_payload(&self, id: &ArtifactId) -> Result<Option<Bytes>>;
    fn has_payload(&self, id: &ArtifactId) -> bool;
}

/// Append-only attention log storage.
/// Maps to: EventLog (per-player file), or iroh Doc (per-player).
pub trait AttentionStore {
    fn append_event(&mut self, event: AttentionSwitchEvent) -> Result<()>;
    fn events(&self, player: &PlayerId) -> Result<Vec<AttentionSwitchEvent>>;
    fn events_since(&self, player: &PlayerId, since: i64) -> Result<Vec<AttentionSwitchEvent>>;
    fn ingest_peer_log(&mut self, peer: PlayerId, events: Vec<AttentionSwitchEvent>) -> Result<()>;
    /// Detect if a peer's log has diverged from our replica (mutual witnessing).
    fn check_integrity(&self, peer: &PlayerId, their_events: &[AttentionSwitchEvent]) -> IntegrityResult;
}

/// Result of checking a peer's attention log against our replica.
pub enum IntegrityResult {
    /// Logs are consistent — theirs extends ours.
    Consistent,
    /// New events appended (normal sync).
    Extended { new_events: usize },
    /// Events were modified or removed — divergence detected.
    Diverged { first_mismatch_index: usize },
    /// We have no prior replica to compare against.
    NoPriorReplica,
}
```

**In-memory implementations** are provided for all three traits, used by the Vault for testing and standalone operation.

**Companion metadata pattern**: Each Leaf blob has immutable content but mutable metadata (steward, audience can change). The `ArtifactStore` handles metadata separately from `PayloadStore`, mirroring how iroh would use a companion Doc alongside a Blob.

---

## Implementation Steps

### Step 1: Scaffold the crate
Create `crates/indras-artifacts/` with `Cargo.toml` and add to workspace members.

**Files to create:**
- `crates/indras-artifacts/Cargo.toml`
- `crates/indras-artifacts/src/lib.rs`

**Add to** root `Cargo.toml` workspace members list.

### Step 2: Storage traits (`store.rs`)

**File:** `crates/indras-artifacts/src/store.rs`

- `ArtifactStore` trait — CRUD for artifact metadata
- `PayloadStore` trait — content-addressed blob storage
- `AttentionStore` trait — append-only log storage + integrity checking
- `IntegrityResult` enum (Consistent, Extended, Diverged, NoPriorReplica)
- `InMemoryArtifactStore` — HashMap-backed implementation
- `InMemoryPayloadStore` — HashMap-backed implementation
- `InMemoryAttentionStore` — Vec-backed implementation with integrity detection

### Step 3: Core types (`artifact.rs`)

**File:** `crates/indras-artifacts/src/artifact.rs`

- `ArtifactId` enum (Blob/Doc) with `bytes()`, `is_blob()`, `is_doc()`, Display, Hash
- `PlayerId` type alias
- `LeafType`, `TreeType` enums (including Vault, Story, Gallery, Document, Request, Exchange, Collection, Custom)
- `ArtifactRef` struct
- `LeafArtifact` struct — NO inline payload, just hash + size + metadata
- `TreeArtifact` struct — references, metadata map, type
- `Artifact` enum (Leaf/Tree)
- `leaf_id(payload: &[u8]) -> ArtifactId` — BLAKE3 hash → ArtifactId::Blob
- `generate_tree_id() -> ArtifactId` — random → ArtifactId::Doc
- `impl Artifact` — `id()`, `steward()`, `audience()`, `is_leaf()`, `is_tree()`, `as_leaf()`, `as_tree()`
- Serde derives for all types

### Step 4: Attention types (`attention.rs`)

**File:** `crates/indras-artifacts/src/attention.rs`

- `AttentionSwitchEvent` struct
- `AttentionLog` — high-level wrapper over `AttentionStore` trait:
  - `new(player, store)` — wraps a store implementation
  - `navigate_to(artifact_id)` — append switch from current_focus to new target. **Navigation IS attention.** No separate `give_attention()` API.
  - `navigate_back(parent_id)` — append switch back to parent (zoom out)
  - `end_session()` — switch to None
  - `current_focus() -> Option<&ArtifactId>` — last event's `to`
  - `dwell_time(artifact_id) -> u64` — sum millis spent on artifact
  - `dwell_times() -> Vec<(ArtifactId, u64)>` — all artifacts ranked
  - `events_since(timestamp) -> &[AttentionSwitchEvent]` — for sync
- `AttentionValue` struct with `heat: f32` (0.0–1.0)
- `compute_heat(artifact_id, peer_logs, audience, now) -> AttentionValue`
  - Filters peer logs to audience members only
  - Recency-weighted: recent switches contribute more heat
  - Normalized to 0.0–1.0 range
  - `now` parameter allows deterministic testing
- **Mutual Witnessing**: `check_peer_integrity(peer, their_events) -> IntegrityResult`
  - Compares incoming peer log against our stored replica
  - Detects if events were removed or modified (divergence)
  - Returns `Diverged { first_mismatch_index }` if tampered
  - The social graph IS the verification layer — no blockchain needed

### Step 5: Peering types (`peering.rs`)

**File:** `crates/indras-artifacts/src/peering.rs`

- `PeerEntry`, `PeerRegistry`, `MutualPeering` structs
- `PeerRegistry::new(player)`
- `PeerRegistry::add_peer(peer_id, display_name) -> Result<()>`
- `PeerRegistry::remove_peer(peer_id) -> Result<()>`
- `PeerRegistry::is_peer(peer_id) -> bool`
- `PeerRegistry::peers() -> &[PeerEntry]`
- `PeerRegistry::peer_count() -> usize`
- `MutualPeering::new(a, b)` — canonical ordering (sorted)

### Step 6: Vault — the player's root artifact (`vault.rs`)

**File:** `crates/indras-artifacts/src/vault.rs`

The Vault is the player's **root Tree Artifact** (TreeType::Vault). It IS an artifact — the player navigates into it. The Vault also provides a local artifact store and operations.

```rust
pub struct Vault<A: ArtifactStore, P: PayloadStore, T: AttentionStore> {
    // The Vault's own Tree Artifact (root of the player's fractal tree)
    pub root: TreeArtifact,
    // Artifact metadata storage (steward, audience, refs, type)
    artifact_store: A,
    // Blob payload storage (content-addressed, lazily loaded)
    payload_store: P,
    // Player's own attention log (append-only)
    attention_store: T,
    // Mutual peer registry
    peer_registry: PeerRegistry,
    // Peer attention log replicas (read-only, ingested from peers)
    peer_attention: HashMap<PlayerId, Vec<AttentionSwitchEvent>>,
}
```

A convenience constructor `Vault::in_memory(player)` creates a Vault with all three in-memory store implementations for testing and standalone use.

**Artifact operations:**
- `place_leaf(payload, leaf_type) -> LeafArtifact` — hash payload, store blob, create LeafArtifact with steward=self. Does NOT auto-add to root tree (caller decides where to compose it).
- `place_tree(tree_type, audience) -> TreeArtifact` — generate random ID, create TreeArtifact with steward=self.
- `get_artifact(id) -> Option<&Artifact>`
- `get_payload(id) -> Option<&Bytes>` — returns None if not yet fetched (lazy loading)
- `store_payload(id, payload)` — cache fetched payload locally
- `compose(tree_id, child_id, position, label) -> Result<()>` — steward-only: add ArtifactRef to Tree
- `remove_ref(tree_id, child_id) -> Result<()>` — steward-only: remove ref
- `set_audience(artifact_id, players) -> Result<()>` — steward-only
- `transfer_stewardship(artifact_id, new_steward) -> Result<()>` — update steward

**Navigation/Attention operations (unified):**
- `navigate_to(artifact_id) -> Result<()>` — switch attention to artifact. Navigation IS the attention event.
- `navigate_back(parent_id) -> Result<()>` — return to parent
- `current_focus() -> Option<&ArtifactId>`
- `heat(artifact_id) -> f32` — compute 0.0–1.0 heat from peer logs + audience
- `attention_value(artifact_id) -> AttentionValue` — full computation

**Peering operations:**
- `peer(peer_id, display_name) -> Result<()>`
- `unpeer(peer_id) -> Result<()>`
- `peers() -> &[PeerEntry]`
- `ingest_peer_log(peer_id, log)` — store a peer's attention log snapshot

**Errors:** `VaultError` enum — NotSteward, ArtifactNotFound, NotATree, AlreadyPeered, NotPeered, PayloadNotLoaded, ExchangeNotFullyAccepted, StoreError(String)

### Step 7: Story convenience (`story.rs`)

**File:** `crates/indras-artifacts/src/story.rs`

A Story is a Tree Artifact (TreeType::Story) representing a sequential journey through artifacts. A conversation is a Story where most leaves are chat messages. A gallery tour is a Story through images. Any sequential experience is a Story.

- `Story` — newtype wrapper around `ArtifactId` (must point to TreeType::Story)
- `Story::create(vault, audience) -> Result<Story>` — create Tree with Story type
- `Story::append(vault, artifact_id) -> Result<()>` — compose artifact at next position
- `Story::send_message(vault, text) -> Result<ArtifactId>` — place Leaf(Message) + append (chat convenience)
- `Story::entries(vault) -> Vec<(ArtifactRef, &Artifact)>` — ordered by position
- `Story::entry_count(vault) -> usize`
- `Story::branch(vault, from_position, audience) -> Result<Story>` — create sub-Story branching from a point, add as ref to parent

### Step 8: Exchange convenience (`exchange.rs`)

**File:** `crates/indras-artifacts/src/exchange.rs`

An Exchange is a Tree Artifact (TreeType::Exchange) representing a negotiation space between two stewards. Contains refs to the two artifacts being discussed + a Story for the negotiation conversation.

- `Exchange` — newtype wrapper around `ArtifactId`
- `Exchange::propose(vault, my_artifact_id, their_artifact_id, audience) -> Result<Exchange>` — create Exchange tree with refs labeled "offered" and "requested", plus an empty Story for negotiation
- `Exchange::conversation(vault) -> Result<Story>` — get the negotiation Story
- `Exchange::offered(vault) -> Option<&Artifact>` — the artifact offered by initiator
- `Exchange::requested(vault) -> Option<&Artifact>` — the artifact requested
- `Exchange::accept(vault) -> Result<()>` — record this party's acceptance in the Exchange tree metadata
- `Exchange::is_accepted_by(vault, player) -> bool` — check if a party has accepted
- `Exchange::complete(vault) -> Result<()>` — execute mutual stewardship transfer. **Atomicity**: both parties must have called `accept()` first. The Exchange doc acts as the coordination point — transfer executes only when both "accept" entries are observed. Fails with `ExchangeNotFullyAccepted` if either party hasn't accepted.

### Step 9: Tests

Comprehensive unit tests in each module.

**Test cases:**

**Artifact types:**
- Leaf ID determinism (same payload = same ArtifactId::Blob)
- Tree ID uniqueness (each generate_tree_id() is different)
- ArtifactId::Blob vs ArtifactId::Doc discrimination
- Serialization round-trip for all types (postcard)

**Audience & Stewardship:**
- Only steward can modify audience
- Only steward can compose/remove refs
- Transfer changes steward — old steward loses control, new steward gains it
- Non-steward operations return NotSteward error

**Composition:**
- Add refs to Tree, ordering by position
- Remove refs from Tree
- Refs preserve labels

**Attention & Heat:**
- navigate_to appends switch events
- navigate_back appends reverse switch
- dwell_time computation from consecutive switches
- current_focus tracks last navigation target
- Heat computation: 0.0 for no peer attention, approaches 1.0 for intense recent activity
- Audience filtering: attention from non-audience members excluded
- Recency weighting: old attention contributes less heat than recent

**Peering:**
- Add/remove peers
- Duplicate detection (AlreadyPeered error)
- MutualPeering canonical ordering

**Vault as root artifact:**
- Vault.root is a TreeArtifact of type Vault
- Player is steward of their own Vault
- Artifacts are composed into the Vault tree

**Story:**
- Create, append mixed artifacts, send messages
- Entries returned in position order
- Branching creates sub-Story

**Mutual Witnessing / Integrity:**
- Consistent: peer log extends our replica cleanly
- Diverged: peer modified earlier events — detected at first mismatch index
- NoPriorReplica: first sync, no comparison possible

**Exchange:**
- Propose creates Exchange with offered/requested refs + conversation Story
- accept() records acceptance, is_accepted_by() checks it
- complete() transfers stewardship both ways only when both accepted
- Fails with ExchangeNotFullyAccepted if either party hasn't accepted
- Fails if either party is not steward of their artifact

**Storage traits:**
- InMemoryArtifactStore: put/get/list round-trips correctly
- InMemoryPayloadStore: store returns Blob(hash), get retrieves content, has_payload works
- InMemoryAttentionStore: append/events/events_since/ingest/integrity all work

---

## Key Design Decisions

1. **Standalone crate** — No dependency on existing `indras-*` crates. Self-contained domain model. Can be integrated with `indras-storage`, `indras-transport`, etc. later.

2. **ArtifactId is an enum (Blob/Doc)** — The variant tells resolution code whether to fetch content by hash (lazy) or open a CRDT document (eager sync). This distinction is fundamental to the lazy-loading architecture.

3. **Vault IS a Tree Artifact** — The player's root is not a special container struct. It's an artifact like any other — with steward, audience, references. The fractal tree starts here.

4. **Navigation IS attention** — There is no separate `give_attention()` API. `navigate_to()` appends an attention switch event. Using the system and generating attention data are the same action.

5. **Lazy payload loading** — LeafArtifact stores hash + size, not inline payload. Payload is fetched on demand when a player navigates to the artifact. The Vault provides `get_payload()` / `store_payload()` for this.

6. **Heat is a 0.0–1.0 float** — Normalized, recency-weighted, UI-ready. Can be used directly as a CSS `--heat` custom property for visual warmth.

7. **Story, not Conversation** — "Story" is the general Tree type for sequential journeys. A conversation is one rendering of a Story. The data model uses Story; the UI spatial grammar renders it as ConversationSpace, GalleryTourSpace, etc.

8. **Request and Exchange as Tree types** — Not special primitives. A Request is a Tree with a central artifact + orbiting offer refs. An Exchange is a Tree with two artifact refs + a negotiation Story. Everything composes from the same building blocks.

9. **Steward-gated operations** — Only the steward can modify audience, transfer stewardship, compose/remove refs in Trees. Enforced at the Vault level.

10. **Content-addressed Leaves** — Leaf ID = BLAKE3 hash. Deduplication is automatic. Same content always produces same ID.

11. **Persistence via traits** — All storage accessed through `ArtifactStore`, `PayloadStore`, `AttentionStore` traits. In-memory implementations for testing; designed to map onto `indras-storage` (RedbStorage, BlobStore, EventLog) or iroh primitives (Docs, Blobs) when integrated.

12. **Mutual witnessing** — Attention log integrity is verified on sync. When ingesting a peer's log, we compare against our stored replica. Divergence (events removed or modified) is detectable and flagged via `IntegrityResult`. The social graph IS the verification layer.

13. **Exchange atomicity** — Both parties write "accept" to the Exchange tree. Stewardship transfer executes only when both acceptances are observed. The Exchange artifact is the coordination point — no separate transaction primitive needed.

---

## File Structure

```
crates/indras-artifacts/
├── Cargo.toml
└── src/
    ├── lib.rs          # Module exports, prelude, re-exports
    ├── artifact.rs     # ArtifactId, Artifact, LeafArtifact, TreeArtifact, types
    ├── store.rs        # Storage traits (ArtifactStore, PayloadStore, AttentionStore) + in-memory impls
    ├── attention.rs    # AttentionSwitchEvent, AttentionLog, AttentionValue, heat computation, integrity
    ├── peering.rs      # PeerEntry, PeerRegistry, MutualPeering
    ├── vault.rs        # Vault<A,P,T> (root artifact + trait-based storage + operations)
    ├── story.rs        # Story convenience (sequential artifact journey)
    ├── exchange.rs     # Exchange convenience (negotiation + stewardship transfer)
    └── error.rs        # VaultError, IntegrityResult enums
```

---

## Verification

1. `cargo check -p indras-artifacts` compiles cleanly
2. `cargo test -p indras-artifacts` — all unit tests pass
3. Key test scenarios:
   - Two Vaults peer with each other, exchange attention logs, compute heat values
   - Story flow: create → append image + send message + append file → read back in order
   - Exchange flow: propose → negotiate via Story → complete transfers stewardship
   - Stewardship transfer: original steward loses control, new steward gains it
   - Audience filtering: heat from non-audience peers excluded
   - Lazy loading: get_payload returns None before store_payload, Some after
   - Vault-as-artifact: root tree has correct type, player is steward
