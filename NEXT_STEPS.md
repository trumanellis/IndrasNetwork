# Next Steps: Running AppFlowy on IndrasNetwork's P2P Layer

## Vision

Replace AppFlowy's centralized cloud sync (WebSocket + Redis + PostgreSQL + S3) with IndrasNetwork's P2P sync layer. The result: **fully decentralized AppFlowy collaboration with zero server infrastructure.**

```
┌──────────────────────────────────────────────────────────────┐
│                      Current AppFlowy                        │
│                                                              │
│  Client A ──WebSocket──→ AppFlowy Cloud ←──WebSocket── Client B  │
│                          (PostgreSQL,                        │
│                           Redis, S3)                         │
└──────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────┐
│                   AppFlowy on IndrasNetwork                   │
│                                                              │
│  Client A ←──iroh/QUIC──→ Client B                           │
│      │         (NAT traversal,          │                    │
│      │          hole punching)          │                    │
│      ▼                                  ▼                    │
│  IndrasNetwork                    IndrasNetwork              │
│  (Yrs sync, encryption,          (Yrs sync, encryption,     │
│   artifact storage,               artifact storage,          │
│   store-and-forward)              store-and-forward)         │
└──────────────────────────────────────────────────────────────┘
```

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Phase 1: CollabPlugin Implementation](#2-phase-1-collabplugin-implementation)
3. [Phase 2: Document Sync Over P2P](#3-phase-2-document-sync-over-p2p)
4. [Phase 3: Awareness Protocol (Presence/Cursors)](#4-phase-3-awareness-protocol)
5. [Phase 4: Folder and Workspace Sync](#5-phase-4-folder-and-workspace-sync)
6. [Phase 5: Database (Spreadsheet) Sync](#6-phase-5-database-sync)
7. [Phase 6: Binary Asset Sync](#7-phase-6-binary-asset-sync)
8. [Phase 7: Access Control and Encryption](#8-phase-7-access-control-and-encryption)
9. [Phase 8: Offline and Multi-Device](#9-phase-8-offline-and-multi-device)
10. [Phase 9: Integration Testing](#10-phase-9-integration-testing)
11. [Phase 10: Distribution Strategy](#11-phase-10-distribution-strategy)
12. [Architecture Deep Dive](#12-architecture-deep-dive)
13. [Open Questions](#13-open-questions)

---

## 1. Prerequisites

Before starting AppFlowy integration, complete these from the migration plan:

- [x] Decision: Replace Automerge with Yrs (this document assumes Yrs migration is done)
- [ ] Complete `MIGRATION_PLAN.md` — Automerge → Yrs migration
- [ ] Verify Yrs version compatibility with AppFlowy-Collab (pin to same `yrs` version)
- [ ] Awareness protocol support in IndrasNetwork

### Version Alignment

AppFlowy-Collab pins specific `yrs` versions. Check their `Cargo.toml`:

```bash
# In AppFlowy-Collab repo
grep "yrs" Cargo.toml
```

IndrasNetwork MUST use the same `yrs` version to ensure binary compatibility of Yrs updates. If AppFlowy uses `yrs = "0.21"`, we use `yrs = "0.21"`. This is non-negotiable — Yrs encoding versions are not guaranteed forward-compatible.

---

## 2. Phase 1: CollabPlugin Implementation

### Goal

Implement AppFlowy's `CollabPlugin` trait backed by IndrasNetwork's P2P transport.

### AppFlowy's Plugin Interface

```rust
// From AppFlowy-Collab
pub trait CollabPlugin: Send + Sync {
    /// Called during Collab initialization with the raw Yrs Doc
    fn init(&self, object_id: &str, origin: &CollabOrigin, doc: &Doc);

    /// Called after initialization is complete
    fn did_init(&self, collab: &Collab, object_id: &str);

    /// Called when any update (local or remote) is applied
    fn receive_update(&self, object_id: &str, txn: &TransactionMut, update: &[u8]);

    /// Called only for locally-originated changes
    fn receive_local_update(&self, origin: &CollabOrigin, object_id: &str, update: &[u8]);

    /// Identifies this plugin's type
    fn plugin_type(&self) -> CollabPluginType;

    /// Cleanup
    fn destroy(&self);
}
```

### Our Implementation

```rust
// crate: indras-appflowy-bridge (new crate)

use appflowy_collab::{CollabPlugin, CollabPluginType, CollabOrigin, Collab};
use indras_network::{Realm, RealmHandle};
use yrs::{Doc, TransactionMut};
use tokio::sync::mpsc;

pub struct IndrasNetworkPlugin {
    /// The IndrasNetwork realm this document belongs to
    realm: RealmHandle,

    /// Channel for sending local updates to the P2P network
    outgoing_tx: mpsc::UnboundedSender<(String, Vec<u8>)>,

    /// Track the object_id to realm mapping
    object_id: String,
}

impl IndrasNetworkPlugin {
    pub fn new(realm: RealmHandle, object_id: String) -> Self {
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();

        // Spawn task that forwards updates to P2P network
        let realm_clone = realm.clone();
        tokio::spawn(async move {
            Self::forward_updates(realm_clone, outgoing_rx).await;
        });

        Self {
            realm,
            outgoing_tx,
            object_id,
        }
    }

    async fn forward_updates(
        realm: RealmHandle,
        mut rx: mpsc::UnboundedReceiver<(String, Vec<u8>)>,
    ) {
        while let Some((object_id, update)) = rx.recv().await {
            // Wrap the Yrs update as an IndrasNetwork event
            // and broadcast to all realm members
            let event = AppFlowyUpdateEvent {
                object_id,
                update,
            };
            realm.broadcast_event(event).await.ok();
        }
    }
}

impl CollabPlugin for IndrasNetworkPlugin {
    fn init(&self, object_id: &str, _origin: &CollabOrigin, doc: &Doc) {
        // Subscribe to incoming P2P updates for this document
        // and apply them to the local Yrs Doc
    }

    fn did_init(&self, collab: &Collab, object_id: &str) {
        // Trigger initial sync — request state from peers
        // Exchange state vectors to catch up on missed changes
    }

    fn receive_update(&self, _object_id: &str, _txn: &TransactionMut, _update: &[u8]) {
        // Called for ALL updates (local + remote)
        // We only care about local updates for forwarding
    }

    fn receive_local_update(&self, _origin: &CollabOrigin, object_id: &str, update: &[u8]) {
        // Forward local changes to P2P network
        self.outgoing_tx
            .send((object_id.to_string(), update.to_vec()))
            .ok();
    }

    fn plugin_type(&self) -> CollabPluginType {
        CollabPluginType::CloudStorage // Only one cloud plugin allowed per Collab
    }

    fn destroy(&self) {
        // Clean up P2P subscriptions
    }
}
```

### New Crate Structure

```
crates/indras-appflowy-bridge/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API
│   ├── plugin.rs           # CollabPlugin implementation
│   ├── sync.rs             # Yrs state vector exchange over P2P
│   ├── awareness.rs        # Awareness protocol bridge
│   ├── document.rs         # AppFlowy document type handlers
│   ├── database.rs         # AppFlowy database type handlers
│   ├── folder.rs           # Workspace hierarchy sync
│   └── assets.rs           # Binary asset (images, files) sync via Artifacts
└── tests/
    ├── sync_test.rs
    └── integration_test.rs
```

### Dependencies

```toml
[dependencies]
indras-network = { path = "../indras-network" }
indras-sync = { path = "../indras-sync" }
yrs = { workspace = true }
y-sync = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
postcard = { workspace = true }

# AppFlowy dependency — either vendored or as git dep
# collab = { git = "https://github.com/AppFlowy-IO/AppFlowy-Collab", branch = "main" }
```

---

## 3. Phase 2: Document Sync Over P2P

### Goal

Sync AppFlowy rich-text documents between peers using IndrasNetwork transport.

### How AppFlowy Documents Work

AppFlowy documents are block trees stored in Yrs:

```
Y.Doc
└── Y.Map("document")
    └── Y.Map("blocks")
        ├── "block_id_1" → Y.Map { type: "paragraph", data: Y.Text("Hello...") }
        ├── "block_id_2" → Y.Map { type: "heading", data: Y.Text("Title"), level: 1 }
        ├── "block_id_3" → Y.Map { type: "image", url: "...", width: 800 }
        └── Y.Array("children_map")  → ordering of blocks
```

### Mapping to IndrasNetwork Artifacts

Each AppFlowy document becomes a **mutable TreeArtifact** in IndrasNetwork:

| AppFlowy Concept | IndrasNetwork Concept |
|------------------|----------------------|
| Workspace | Realm |
| Document (Y.Doc) | TreeArtifact (mutable, Yrs-backed) |
| Document page | Document within Realm |
| Image/attachment | LeafArtifact (immutable, content-addressed) |
| Folder hierarchy | TreeArtifact referencing other artifacts |

### Sync Protocol

```
Peer Zephyr                              Peer Nova
    |                                        |
    | [Opens document "Project Plan"]        |
    |                                        |
    |-- state_vector(doc_id) -------------->|
    |                                        | compute diff
    |<-- update(missing_changes) -----------|
    |<-- state_vector(doc_id) --------------|
    |                                        |
    | compute diff                           |
    |-- update(missing_changes) ----------->|
    |                                        |
    | [Both in sync]                         |
    |                                        |
    | [Zephyr types "Hello"]                 |
    |-- incremental_update ---------------->|  ← real-time, sub-second
    |                                        |
    |           [Nova types "World"]         |
    |<-- incremental_update ----------------|  ← Yrs CRDT auto-merges
```

### Implementation Steps

1. **Map object_id to Realm document** — When AppFlowy opens a document by `object_id`, look up or create the corresponding IndrasNetwork Realm document.

2. **Initial sync on open** — Exchange state vectors with all online peers in the Realm. Apply any missing updates.

3. **Real-time forwarding** — `receive_local_update()` sends each Yrs update to all Realm members immediately via iroh.

4. **Incoming update application** — Listen for P2P events, decode Yrs updates, apply to local Doc via `transact_mut().apply_update()`.

5. **Store-and-forward for offline peers** — When a peer comes online, IndrasNetwork's existing store-and-forward system delivers queued updates.

---

## 4. Phase 3: Awareness Protocol

### Goal

Enable real-time cursor positions, selections, and online presence indicators.

### How Awareness Works

Awareness is a lightweight, ephemeral protocol separate from document CRDTs:

```rust
use y_sync::awareness::{Awareness, AwarenessUpdate};

// Each peer maintains local presence state
awareness.set_local_state(serde_json::to_string(&json!({
    "user": {
        "name": "Zephyr",
        "color": "#e74c3c"
    },
    "cursor": {
        "block_id": "abc123",
        "offset": 42
    },
    "selection": {
        "start": { "block_id": "abc123", "offset": 40 },
        "end": { "block_id": "abc123", "offset": 50 }
    }
}))?);

// Awareness updates are broadcast to all peers
// They expire after 30 seconds if not refreshed
```

### P2P Awareness Transport

```rust
// In indras-appflowy-bridge/src/awareness.rs

pub struct AwarenessBridge {
    awareness: Awareness,
    realm: RealmHandle,
}

impl AwarenessBridge {
    /// Broadcast local awareness state to peers
    pub async fn broadcast(&self) -> Result<()> {
        let update = self.awareness.update()?;
        let encoded = update.encode_v1();
        self.realm.broadcast_ephemeral(
            EphemeralEvent::Awareness(encoded)
        ).await
    }

    /// Apply awareness update from remote peer
    pub fn apply_remote(&mut self, bytes: &[u8]) -> Result<()> {
        let update = AwarenessUpdate::decode_v1(bytes)?;
        self.awareness.apply_update(update)?;
        Ok(())
    }

    /// Get all peer states (for UI rendering)
    pub fn peer_states(&self) -> HashMap<ClientId, String> {
        self.awareness.get_states().clone()
    }
}
```

### Transport Considerations

Awareness is **ephemeral** — it should NOT use store-and-forward. If a peer is offline, their cursor state is irrelevant. Use a separate "fire-and-forget" channel:

- **Document updates:** Reliable, store-and-forward, persistent
- **Awareness updates:** Unreliable, best-effort, ephemeral

IndrasNetwork's iroh transport supports both patterns. Use unreliable datagrams for awareness to minimize bandwidth.

---

## 5. Phase 4: Folder and Workspace Sync

### Goal

Sync the AppFlowy workspace hierarchy (which pages exist, their names, their arrangement).

### AppFlowy Folder Structure

```rust
// Simplified AppFlowy folder representation in Yrs
Y.Doc("workspace_folder")
├── Y.Map("views")
│   ├── "view_id_1" → Y.Map {
│   │     name: "Project Plan",
│   │     layout: "document",
│   │     icon: "📄",
│   │     parent_view_id: null,
│   │     children: Y.Array ["view_id_3", "view_id_4"]
│   │   }
│   ├── "view_id_2" → Y.Map {
│   │     name: "Task Tracker",
│   │     layout: "grid",
│   │     icon: "📊",
│   │     parent_view_id: null,
│   │     children: Y.Array []
│   │   }
│   └── ...
├── Y.Map("meta")
│   └── current_view: "view_id_1"
└── Y.Map("trash")
    └── ...
```

### Mapping to IndrasNetwork

The workspace folder is a single Yrs document that tracks the tree of all pages. It maps to a **Realm-level metadata document**:

```
IndrasNetwork Realm (= AppFlowy Workspace)
├── Realm Metadata Document (Yrs)
│   └── Contains the folder hierarchy (views, names, layout types)
├── Document "view_id_1" (Yrs)  → Project Plan content
├── Document "view_id_2" (Yrs)  → Task Tracker content
├── LeafArtifact (image1.png)   → Attached image
└── LeafArtifact (report.pdf)   → Attached file
```

### Implementation

1. **Workspace = Realm** — Creating/joining a workspace is creating/joining a Realm.
2. **Folder doc** — One dedicated Yrs document per Realm holds the folder structure. All peers sync this document.
3. **Lazy document loading** — Individual page documents are only synced when opened, not when the workspace loads. Exchange state vectors lazily.
4. **Conflict resolution** — Yrs handles concurrent folder edits (rename, move, create) automatically via CRDT semantics.

---

## 6. Phase 5: Database (Spreadsheet) Sync

### Goal

Sync AppFlowy database views (Grid, Board, Calendar) between peers.

### AppFlowy Database Structure

```
Y.Doc("database_<id>")
├── Y.Map("fields")
│   ├── "field_1" → Y.Map { name: "Task", field_type: "RichText" }
│   ├── "field_2" → Y.Map { name: "Status", field_type: "SingleSelect",
│   │                        options: [...] }
│   └── "field_3" → Y.Map { name: "Due Date", field_type: "DateTime" }
├── Y.Map("rows")
│   ├── "row_1" → Y.Map {
│   │     cells: Y.Map {
│   │       "field_1": "Design login page",
│   │       "field_2": "In Progress",
│   │       "field_3": "2026-02-20"
│   │     }
│   │   }
│   └── ...
└── Y.Array("views")
    ├── Y.Map { id: "grid_view_1", layout: "Grid", filters: [...], sorts: [...] }
    └── Y.Map { id: "board_view_1", layout: "Board", group_field: "field_2" }
```

### Sync Strategy

Databases sync identically to documents — they are Yrs documents with a specific schema. The `CollabPlugin` receives updates the same way. No special handling needed at the transport level.

**Concurrency scenarios:**
- Two peers add rows simultaneously → Yrs merges both (rows appear in both)
- Two peers edit the same cell → Last-writer-wins (Yrs Map semantics)
- One peer adds a column while another adds a row → Both changes merge cleanly

---

## 7. Phase 6: Binary Asset Sync

### Goal

Sync images, file attachments, and other binary assets that AppFlowy documents reference.

### The Artifact Advantage

This is where IndrasNetwork shines. AppFlowy Cloud uses S3 for binary storage. We use **content-addressed LeafArtifacts**:

```
AppFlowy Document
└── Image block: { url: "indras://blake3:abc123def..." }
                              │
                              ▼
                    IndrasNetwork LeafArtifact
                    ├── ID: BLAKE3 hash of content
                    ├── Content: raw image bytes
                    ├── Encrypted: per-artifact key
                    └── Deduplicated: same image = same hash
```

### Implementation

1. **Custom URL scheme** — AppFlowy image/file URLs use `indras://` scheme pointing to artifact hashes instead of HTTP URLs.

2. **Upload flow:**
   ```
   User pastes image → Create LeafArtifact(image_bytes)
                      → Get artifact_id (BLAKE3 hash)
                      → Insert into Yrs doc: { url: "indras://<hash>" }
                      → Share artifact with Realm peers
   ```

3. **Download flow:**
   ```
   Peer opens document → Sees image block with indras:// URL
                       → Request artifact from Realm peers
                       → Download via iroh (QUIC, parallel chunks)
                       → Cache locally in blob storage
                       → Render in UI
   ```

4. **Deduplication** — Same image pasted in 10 documents = stored once (content-addressed).

5. **Lazy loading** — Assets are fetched on-demand when a document containing them is opened. Not eagerly synced.

6. **Encryption** — Each artifact has its own encryption key. Revoking a user's access = revoking their artifact keys. The image bytes are never exposed to peers without access.

---

## 8. Phase 7: Access Control and Encryption

### Goal

Map IndrasNetwork's per-artifact encryption and access control to AppFlowy's sharing model.

### Access Model Mapping

| AppFlowy Concept | IndrasNetwork Concept |
|------------------|----------------------|
| Workspace member | Realm member |
| Document sharing | Artifact AccessGrant |
| View-only access | `AccessMode::ReadOnly` |
| Edit access | `AccessMode::ReadWrite` |
| Admin/owner | `AccessMode::Admin` |
| Revoke access | Revoke artifact encryption key |

### Sharing Flow

```
Zephyr wants to share "Project Plan" with Nova:

1. Zephyr → IndrasNetwork: grant_access(document_artifact, Nova.peer_id, ReadWrite)
2. IndrasNetwork:
   a. Generate document encryption key (or use existing)
   b. Encrypt key with Nova's public key
   c. Store AccessGrant in Realm metadata
   d. Notify Nova via store-and-forward
3. Nova joins → receives encrypted key → can now decrypt and sync document
```

### End-to-End Encryption

All AppFlowy content is encrypted at the IndrasNetwork layer:

- **Document updates** (Yrs binary) → encrypted with document key before P2P transit
- **Binary assets** (images/files) → encrypted with artifact key
- **Folder metadata** → encrypted with workspace key
- **Awareness data** → NOT encrypted (ephemeral, low sensitivity)

AppFlowy never sees plaintext on the wire. The P2P transport only carries encrypted bytes.

---

## 9. Phase 8: Offline and Multi-Device

### Goal

Ensure AppFlowy works fully offline and syncs across a user's devices.

### Offline Behavior

IndrasNetwork already supports this via store-and-forward:

1. **User goes offline** — All edits are saved locally to Yrs Doc + persisted to disk via existing storage layer.
2. **User comes back online** — IndrasNetwork exchanges state vectors with peers, applies missed updates.
3. **Queued updates from peers** — Store-and-forward holds updates from other peers until this user acknowledges receipt.

### Multi-Device via HomeRealm

A user's personal HomeRealm can sync their AppFlowy workspace across their own devices:

```
Zephyr's Phone        Zephyr's Laptop       Zephyr's Desktop
     │                     │                      │
     └─── HomeRealm (Zephyr's private sync) ──────┘
          ├── Workspace folder document
          ├── All personal documents
          ├── All personal databases
          └── All personal assets
```

This happens automatically — the HomeRealm is IndrasNetwork's existing mechanism for personal data sync. AppFlowy content is just another set of artifacts in the HomeRealm.

### Conflict Resolution for Multi-Device

Same user edits the same document on two devices while offline:

1. Both devices make changes to the local Yrs Doc
2. When devices reconnect, state vectors are exchanged
3. Yrs CRDT merges both sets of changes automatically
4. No conflicts, no data loss — both devices converge

---

## 10. Phase 9: Integration Testing

### Test Scenarios

#### Scenario 1: Two-Peer Document Collaboration
```
1. Zephyr creates a new document "Sprint Planning"
2. Nova joins Zephyr's workspace
3. Both open the document simultaneously
4. Zephyr types a heading, Nova types a paragraph
5. Verify: Both see each other's changes within 1 second
6. Verify: Document structure is consistent on both peers
```

#### Scenario 2: Offline Editing and Merge
```
1. Zephyr and Nova both have "Budget Spreadsheet" open
2. Nova goes offline
3. Zephyr adds 3 rows, changes a cell
4. Nova adds 2 rows, changes a different cell
5. Nova comes back online
6. Verify: All 5 new rows exist on both peers
7. Verify: Both cell changes are preserved
```

#### Scenario 3: Binary Asset Sharing
```
1. Zephyr pastes an image into a document
2. Verify: Image is stored as LeafArtifact
3. Nova opens the document
4. Verify: Nova can download and render the image
5. Verify: Image is not re-downloaded if already cached
```

#### Scenario 4: Access Revocation
```
1. Zephyr shares workspace with Nova (ReadWrite) and Sage (ReadOnly)
2. Sage can open and read all documents
3. Sage cannot edit documents (ReadOnly enforcement)
4. Zephyr revokes Sage's access
5. Verify: Sage can no longer decrypt new updates
6. Verify: Nova still has full access
```

#### Scenario 5: Three-Peer Mesh
```
1. Zephyr, Nova, and Orion collaborate on a document
2. Each makes concurrent edits
3. Zephyr loses connection to Nova but stays connected to Orion
4. Orion relays changes between Zephyr and Nova
5. Verify: All three peers converge despite partial connectivity
```

#### Scenario 6: Large Document Performance
```
1. Create a document with 1000 blocks
2. Two peers edit different sections simultaneously
3. Verify: Sync completes within 2 seconds
4. Verify: No dropped updates
5. Measure: State vector size, update size, round-trip latency
```

### Test Infrastructure

```bash
# Run bridge integration tests
cargo test -p indras-appflowy-bridge

# Run with simulated network conditions
cargo test -p indras-appflowy-bridge --features simulate-latency

# Run stress tests
cargo test -p indras-appflowy-bridge --test stress -- --ignored
```

---

## 11. Phase 10: Distribution Strategy

### How Users Get This

Three options, from least to most ambitious:

#### Option A: AppFlowy Plugin (Lowest friction)

Ship `indras-appflowy-bridge` as a plugin that users install into stock AppFlowy.

- **Pro:** Users keep their familiar AppFlowy UI
- **Pro:** Can be distributed via AppFlowy's plugin system
- **Con:** Limited by AppFlowy's plugin API surface
- **Con:** Dependent on AppFlowy's release cycle

#### Option B: Custom AppFlowy Build

Fork AppFlowy and replace the cloud sync layer with IndrasNetwork.

- **Pro:** Full control over the integration
- **Pro:** Can optimize the entire stack
- **Con:** Must maintain the fork, track upstream changes
- **Con:** Higher distribution burden

#### Option C: IndrasNetwork SDK + AppFlowy as Reference App

Position IndrasNetwork as a P2P collaboration SDK. AppFlowy is the first app built on it, but not the last.

- **Pro:** Platform play — attract more apps to the ecosystem
- **Pro:** IndrasNetwork SDK can be used by any Yjs-compatible app
- **Con:** Most work upfront
- **Con:** SDK design is harder than a single integration

**Recommendation:** Start with **Option B** (custom build) to prove the integration works end-to-end. Then extract the reusable parts into **Option C** (SDK) for the ecosystem play.

---

## 12. Architecture Deep Dive

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        AppFlowy UI (Flutter)                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   Editor      │  │   Database   │  │   Folder Browser     │  │
│  │   (Blocks)    │  │   (Grid/     │  │   (Workspace tree)   │  │
│  │              │  │    Board)     │  │                      │  │
│  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘  │
│         │                 │                      │              │
│  ┌──────▼─────────────────▼──────────────────────▼───────────┐  │
│  │                    AppFlowy-Collab                         │  │
│  │  ┌─────────────────────────────────────────────────────┐  │  │
│  │  │                 Collab (Yrs Doc)                     │  │  │
│  │  │  ┌─────────────────┐  ┌──────────────────────────┐  │  │  │
│  │  │  │ RocksDB Plugin  │  │ IndrasNetwork Plugin     │  │  │  │
│  │  │  │ (local disk)    │  │ (P2P sync)               │  │  │  │
│  │  │  └─────────────────┘  └──────────┬───────────────┘  │  │  │
│  │  └──────────────────────────────────┼───────────────────┘  │  │
│  └─────────────────────────────────────┼─────────────────────┘  │
└────────────────────────────────────────┼────────────────────────┘
                                         │
                    ┌────────────────────▼────────────────────┐
                    │        indras-appflowy-bridge           │
                    │                                         │
                    │  ┌──────────┐  ┌────────────────────┐  │
                    │  │ Sync     │  │ Awareness Bridge   │  │
                    │  │ Manager  │  │ (cursors/presence) │  │
                    │  └────┬─────┘  └────────┬───────────┘  │
                    └───────┼─────────────────┼──────────────┘
                            │                 │
                    ┌───────▼─────────────────▼──────────────┐
                    │           IndrasNetwork Core            │
                    │                                         │
                    │  ┌──────────┐  ┌──────────────────┐   │
                    │  │  Realm   │  │  Artifact Store  │   │
                    │  │  (Yrs    │  │  (LeafArtifact   │   │
                    │  │   sync)  │  │   for images)    │   │
                    │  └────┬─────┘  └──────────────────┘   │
                    │       │                                │
                    │  ┌────▼───────────────────────────┐   │
                    │  │  iroh Transport (QUIC)          │   │
                    │  │  NAT traversal, hole punching   │   │
                    │  │  Encrypted channels              │   │
                    │  └────────────────────────────────┘   │
                    └───────────────────────────────────────┘
```

### Data Flow: Local Edit

```
1. User types "Hello" in AppFlowy editor
2. AppFlowy-Collab creates Yrs transaction
3. Transaction commits → Yrs update bytes generated
4. CollabPlugin.receive_local_update(update_bytes) fires
5. IndrasNetworkPlugin sends update to realm.broadcast_event()
6. iroh encrypts and transmits to all online peers
7. Store-and-forward queues for offline peers
```

### Data Flow: Remote Edit

```
1. iroh receives encrypted bytes from peer
2. IndrasNetwork decrypts, identifies as AppFlowy update event
3. indras-appflowy-bridge receives event
4. Decodes Yrs Update from event payload
5. Applies to local Y.Doc via transact_mut().apply_update()
6. AppFlowy-Collab observers fire → UI updates
```

### Data Flow: Initial Sync (Peer Joins)

```
1. New peer joins Realm
2. indras-appflowy-bridge sends local state_vector for each open document
3. Existing peers compute diff: encode_state_as_update_v1(new_peer_sv)
4. Send missing updates to new peer
5. New peer applies updates, sends their own state_vector back
6. Existing peers apply any missing updates from new peer
7. All peers converged
```

---

## 13. Open Questions

### Technical

1. **AppFlowy-Collab as dependency** — Should we vendor AppFlowy-Collab into IndrasNetwork, or use it as a git dependency? Vendoring gives stability, git dep gives automatic updates.

2. **Yrs version pinning** — How do we handle AppFlowy upgrading their Yrs version? We need a compatibility matrix and migration strategy for Yrs wire format changes.

3. **Flutter FFI** — AppFlowy's UI is Flutter. The bridge crate is Rust. We need FFI bindings (probably via `flutter_rust_bridge`) to connect them. How much of AppFlowy's existing FFI infrastructure can we reuse?

4. **Document ID mapping** — AppFlowy uses UUIDs for document IDs. IndrasNetwork uses BLAKE3 hashes for immutable artifacts and random 32-byte IDs for mutable ones. Need a stable mapping between the two ID spaces.

5. **Deletion semantics** — When a user deletes a document, what happens? In AppFlowy Cloud, the server handles it. In P2P, we need consensus on deletion. Options: tombstone in folder doc, or "trash" is just a folder.

### Strategic

6. **AppFlowy team engagement** — Should we approach the AppFlowy team about official P2P support? They have open issues requesting it ([#4562](https://github.com/AppFlowy-IO/AppFlowy/issues/4562)). Contributing upstream vs. maintaining a fork.

7. **Other Yjs apps** — After AppFlowy, what's the next target? AFFiNE, BlockSuite, Hocuspocus ecosystem? The bridge pattern should be reusable.

8. **Relay nodes** — For users behind strict NATs where hole-punching fails, should IndrasNetwork offer optional relay nodes? iroh supports this, but it introduces a server dependency.

9. **Mobile support** — AppFlowy has iOS and Android apps. iroh works on mobile, but battery life and background sync are challenges. What's the mobile story?

---

## Milestone Summary

| Milestone | Description | Dependencies |
|-----------|-------------|--------------|
| **M0** | Complete Automerge → Yrs migration | `MIGRATION_PLAN.md` |
| **M1** | `CollabPlugin` trait implementation | M0 |
| **M2** | Single-document P2P sync working | M1 |
| **M3** | Awareness protocol (cursors/presence) | M2 |
| **M4** | Folder/workspace sync | M2 |
| **M5** | Database (Grid/Board) sync | M2 |
| **M6** | Binary asset sync via Artifacts | M2 |
| **M7** | Access control and E2E encryption | M2 |
| **M8** | Offline sync and multi-device | M2 |
| **M9** | Integration test suite passing | M2-M8 |
| **M10** | Distribution (custom AppFlowy build) | M9 |
