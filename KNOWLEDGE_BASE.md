# IndrasNetwork Knowledge Base

A comprehensive reference for understanding IndrasNetwork and SyncEngine — a peer-to-peer collaboration platform built on the philosophy of Indra's Net.

---

## Table of Contents

1. [Project Overview & Philosophy](#1-project-overview--philosophy)
2. [Architecture Layers](#2-architecture-layers)
3. [Core Concepts Deep Dive](#3-core-concepts-deep-dive)
4. [SyncEngine Domain Types](#4-syncengine-domain-types)
5. [Data Flow Diagrams](#5-data-flow-diagrams)
6. [Key Design Decisions](#6-key-design-decisions)
7. [Technology Stack](#7-technology-stack)
8. [Code Patterns & Examples](#8-code-patterns--examples)
9. [Glossary](#9-glossary)

---

## 1. Project Overview & Philosophy

### The Indra's Net Metaphor

IndrasNetwork draws its name and philosophy from Indra's Net — a concept from Buddhist and Hindu traditions describing an infinite cosmic net where a jewel sits at each vertex. Every jewel reflects all other jewels, creating an infinite web of mutual reflection. There is no center, no hierarchy — just equal nodes reflecting one another.

This metaphor guides the entire architecture:

- **No Central Coordinator**: Every peer is equal. There are no special servers, no master nodes, no privileged positions in the network.
- **Mutual Reflection**: Peers discover each other through gossip, naturally forming connections based on shared interests (realms) rather than geographic or administrative boundaries.
- **Offline-First**: The network assumes disconnection is normal. Peers store messages for offline contacts and deliver them when connections resume.
- **Subjective Trust**: Rather than global reputation scores, each peer maintains their own subjective view of trust and sentiment toward others.

### Core Principles

1. **Identity IS Connection**: Your cryptographic identity (public key) is your network address. Anyone who knows your identity can send you messages.

2. **Realms as Collaborative Spaces**: Groups form around "realms" — named collaborative spaces with automatic CRDT synchronization. No one "owns" a realm; all members are equal participants.

3. **Gossip-Based Discovery**: Peers find each other through topic-based gossip. When you join a realm, you automatically discover and connect to other members.

4. **Store-and-Forward Routing**: Messages for offline peers are held by mutual contacts and delivered when the recipient comes online.

5. **Post-Quantum Ready**: The cryptographic layer uses ML-KEM (Kyber) and ML-DSA (Dilithium) algorithms resistant to quantum computing attacks.

### Greenfield Status

This is a greenfield project with no backward compatibility constraints. Modules can be freely deleted, replaced, or rewritten without preserving old interfaces. The codebase evolves rapidly toward the cleanest possible design.

---

## 2. Architecture Layers

IndrasNetwork is organized as a Rust workspace with clearly separated concerns:

| Layer | Crate | Purpose |
|-------|-------|---------|
| **Foundation** | `indras-core` | Traits, types, generic identity (`Packet<I>`, `RoutingDecision`, `PeerIdentity`) |
| **Crypto** | `indras-crypto` | Post-quantum cryptography (ML-KEM-768, ML-DSA-65, ChaCha20-Poly1305) |
| **Transport** | `indras-transport` | Iroh/QUIC connections, gossip discovery, connection management |
| **Storage** | `indras-storage` | Tri-layer: append logs, redb KV, content-addressed blobs |
| **Routing** | `indras-routing` | Store-and-forward router, mutual peer tracking, back-propagation |
| **DTN** | `indras-dtn` | Delay-tolerant networking strategies (Epidemic, Spray-and-Wait) |
| **Sync** | `indras-sync` | Automerge CRDT integration, document synchronization |
| **Gossip** | `indras-gossip` | Topic-based gossip protocol, peer discovery |
| **Node** | `indras-node` | High-level coordinator combining all layers |
| **Network SDK** | `indras-network` | Developer-facing API (`IndrasNetwork`, `Realm`, `Document`, `Artifact`) |
| **App Layer** | `indras-sync-engine` | Domain types (Quest, Blessing, Token, Attention, Humanness) |
| **UI** | `indras-genesis`, `indras-ui` | Dioxus desktop application components |

### Layer Interaction Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                        indras-genesis (UI)                       │
├─────────────────────────────────────────────────────────────────┤
│                      indras-sync-engine (App)                    │
│         Quest, Blessing, Token, Attention, Humanness             │
├─────────────────────────────────────────────────────────────────┤
│                       indras-network (SDK)                       │
│            IndrasNetwork, Realm, Document, Artifact              │
├─────────────────────────────────────────────────────────────────┤
│                        indras-node                               │
│                    High-level coordinator                        │
├────────────────┬────────────────┬────────────────┬──────────────┤
│ indras-routing │  indras-sync   │ indras-gossip  │  indras-dtn  │
├────────────────┴────────────────┴────────────────┴──────────────┤
│                       indras-storage                             │
│              Append logs + redb KV + Content blobs               │
├─────────────────────────────────────────────────────────────────┤
│                      indras-transport                            │
│                     Iroh/QUIC connections                        │
├─────────────────────────────────────────────────────────────────┤
│                       indras-crypto                              │
│              ML-KEM-768, ML-DSA-65, ChaCha20-Poly1305            │
├─────────────────────────────────────────────────────────────────┤
│                        indras-core                               │
│                 Traits, types, generic identity                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. Core Concepts Deep Dive

### 3.1 Identity Model

**MemberId** is the fundamental identity type — a 32-byte Ed25519 public key that doubles as a network address.

```rust
pub type MemberId = [u8; 32];  // Ed25519 public key
```

Key properties:

- **Identity IS Address**: Anyone who knows your MemberId can compute your inbox realm ID and send you connection requests.
- **Deterministic Derivation**: All system realms (HomeRealm, ContactsRealm, Inbox, DM realms) are deterministically derived from MemberId(s).
- **No Username Registration**: There's no central registry. Your identity is self-sovereign.

**Derivation Examples:**

```rust
// Home realm: BLAKE3("home-v1:" + member_id)
pub fn home_realm_id(member_id: MemberId) -> RealmId;

// Inbox realm: BLAKE3("inbox-v1:" + member_id)
pub fn inbox_realm_id(member_id: MemberId) -> RealmId;

// DM realm: BLAKE3("dm-v1:" + sorted(member_a, member_b))
pub fn dm_realm_id(a: MemberId, b: MemberId) -> RealmId;
```

**Post-Quantum Identity**: The `PQIdentity` type wraps ML-DSA-65 (Dilithium) signatures for quantum-resistant authentication.

### 3.2 Realm Model

Realms are collaborative spaces where members share documents, messages, and artifacts. Every realm has a unique 32-byte ID.

**Realm Types:**

| Type | ID Derivation | Purpose |
|------|---------------|---------|
| **Regular Realm** | Random UUID | Invitation-based collaboration |
| **HomeRealm** | `BLAKE3("home-v1:" + member_id)` | Personal storage, always exists |
| **ContactsRealm** | `BLAKE3("contacts-v1:" + member_id)` | Contact list management |
| **Inbox** | `BLAKE3("inbox-v1:" + member_id)` | Connection request notifications |
| **DM Realm** | `BLAKE3("dm-v1:" + sorted_peers)` | Private messaging between two peers |
| **Peer-Set Realm** | `BLAKE3("group-v1:" + sorted_peers)` | Deterministic group from peer set |

**Invitation Flow:**

1. Creator generates realm with random ID
2. Creator shares `InviteCode` (realm ID + encryption key)
3. Recipient joins via invite code
4. Both peers discover each other through gossip

**DM Realm Properties:**

- Same pair of members always produces same realm ID (order-independent)
- Deterministic key derivation means both peers compute identical encryption keys
- No "friend request" flow — just direct connection via inbox notification

### 3.3 Document Synchronization

Documents are typed, CRDT-backed data structures using Automerge. They automatically synchronize across all realm members.

```rust
pub struct Document<T: DocumentSchema> {
    realm_id: RealmId,
    name: String,
    state: Arc<RwLock<T>>,
    // ...
}
```

**Synchronization Model:**

1. **Local Update**: `doc.update(|state| { ... })` modifies local state
2. **Broadcast**: Change is serialized and sent to all realm members via gossip
3. **Remote Merge**: Recipients deserialize and merge using CRDT semantics
4. **Derived State**: Complex state (like attention totals) is rebuilt after merge

**Dual Sync Architecture:**

- **Events**: Store-and-forward for reliable delivery (append-only logs)
- **Documents**: Full CRDT sync for complex data structures (Automerge)

**Convergence via Deterministic Ordering:**

Events are ordered by `(timestamp_millis, event_id)` tuple, ensuring all peers converge to identical state regardless of receipt order.

### 3.4 Store-and-Forward Routing

The router implements a four-step decision process:

```
┌─────────────────────────────────────────────────────────────┐
│                    Routing Decision                          │
├─────────────────────────────────────────────────────────────┤
│  1. DIRECT: Destination online + directly connected         │
│     → Deliver immediately                                    │
│                                                              │
│  2. HOLD: Destination offline + directly connected           │
│     → Store for later delivery when they come online         │
│                                                              │
│  3. RELAY: Not directly connected                            │
│     → Find mutual peers who connect to both                  │
│     → Send through relay for store-and-forward               │
│                                                              │
│  4. DROP: No route available                                 │
│     → TTL expired or no mutual peers                         │
└─────────────────────────────────────────────────────────────┘
```

**Mutual Peer Tracking:**

The router maintains a cache of "mutual peers" — peers who are connected to both sender and recipient. These serve as natural relay points.

**Back-Propagation ACKs:**

When a message is finally delivered, an acknowledgment propagates back through the relay chain, confirming delivery to the original sender.

**Packet Structure:**

```rust
pub struct Packet<I: PeerIdentity> {
    pub id: PacketId,
    pub source: I,
    pub destination: I,
    pub payload: EncryptedPayload,
    pub routing_hints: Vec<I>,    // Suggested relays
    pub created_at: DateTime<Utc>,
    pub ttl: u8,                  // Hops before dropping (default: 10)
    pub visited: HashSet<u64>,    // Prevents loops
    pub priority: Priority,
    pub correlation_id: Option<Uuid>,
}
```

### 3.5 Encryption Model

**Per-Interface Symmetric Keys:**

Each realm has a symmetric key (ChaCha20-Poly1305) shared among all members. This key encrypts all messages within the realm.

**DM Key Exchange:**

- Both peers derive the same key seed: `BLAKE3("dm-key-v1:" + sorted_peers)`
- For stronger security, ML-KEM key exchange can establish forward-secret keys

**Per-Artifact Encryption:**

Each artifact (file) has its own encryption key, stored in an `ArtifactKeyRegistry`. Keys can be:
- Shared with specific members
- Revoked (tombstoned) to prevent future access
- Recovered through backup mechanisms

---

## 4. SyncEngine Domain Types

SyncEngine is the first application built on IndrasNetwork, providing collaboration primitives for coordinated work.

### 4.1 Quest System

Quests are lightweight collaboration intentions with proof-of-service verification.

```rust
pub struct Quest {
    pub id: QuestId,              // 16-byte unique ID
    pub title: String,
    pub description: String,
    pub image: Option<ArtifactId>,
    pub creator: MemberId,
    pub claims: Vec<QuestClaim>,
    pub created_at_millis: i64,
    pub completed_at_millis: Option<i64>,
    pub deadline_millis: Option<i64>,
    pub priority: QuestPriority,  // Low, Normal, High, Urgent
}

pub struct QuestClaim {
    pub claimant: MemberId,
    pub proof: Option<ArtifactId>,
    pub proof_folder: Option<ProofFolderId>,
    pub submitted_at_millis: i64,
    pub verified: bool,
    pub verified_at_millis: Option<i64>,
}
```

**Quest Workflow:**

1. **Create**: Creator posts quest with title, description, optional deadline
2. **Claim**: Multiple members can submit claims with proof artifacts
3. **Verify**: Creator reviews and verifies valid claims
4. **Complete**: Creator marks quest complete

**Key Properties:**

- Multiple claimants per quest (collaborative work)
- Proof artifacts or proof folders for accountability
- CRDT-synchronized across all realm members

### 4.2 Attention & Blessing

**Attention** tracks time spent on quests via focus-switch events:

```rust
pub struct AttentionSwitchEvent {
    pub member: MemberId,
    pub from_quest: Option<QuestId>,
    pub to_quest: Option<QuestId>,
    pub timestamp_millis: i64,
}
```

Time on a quest = difference between switch events. Attention accumulates until "blessed."

**Blessing** releases accumulated attention as validation:

```rust
pub struct Blessing {
    pub id: BlessingId,
    pub blesser: MemberId,
    pub blessed: MemberId,
    pub quest_id: QuestId,
    pub claim_id: ClaimId,
    pub attention_millis: u64,
    pub timestamp_millis: i64,
}
```

When a quest creator verifies a claim, they can bless the claimant, converting their attention into a "Token of Gratitude."

### 4.3 Tokens of Gratitude

Tokens are transferable appreciation with subjective valuation:

```rust
pub struct TokenOfGratitude {
    pub id: TokenOfGratitudeId,
    pub current_steward: MemberId,
    pub steward_chain: Vec<MemberId>,  // Transfer history
    pub blesser: MemberId,
    pub blessing_id: BlessingId,
    pub quest_id: QuestId,
    pub state: TokenState,  // Held, Pledged, Released
}
```

**Subjective Valuation Formula:**

```
subjective_value = raw_attention_millis × trust_chain_weight × humanness_freshness
```

Where:
- `raw_attention_millis`: Objective time backing the token
- `trust_chain_weight`: Observer's sentiment toward steward chain (decays 0.7× per hop)
- `humanness_freshness`: How recently the blesser was attested as human

**Why Subjective?**

The same token has different values to different observers. If you don't trust anyone in the steward chain, the token is worth zero to you. This makes Sybil-minted tokens invisible to honest participants.

### 4.4 Sentiment & Trust

**Direct Sentiment:**

Each contact has a sentiment score: -1 (blocked), 0 (neutral), or +1 (trusted), with a "relayable" flag.

**Relayed Sentiment (2nd Degree):**

Contacts publish their relayable sentiments. You can see what your contacts think about their contacts, attenuated by 0.3×.

```rust
pub struct SentimentView {
    pub direct: Vec<(MemberId, i8)>,     // From your contacts
    pub relayed: Vec<RelayedSentiment>,  // From contacts' contacts
}
```

**Weighted Score:**

```rust
score = (direct_sum + relayed_sum × 0.3) / (direct_count + relayed_count × 0.3)
```

### 4.5 Humanness Attestation

Humanness is not a one-time credential but a heartbeat that decays over time.

**Freshness Model:**

- Full freshness (1.0) for 7 days after attestation
- Exponential decay after: `e^(-0.1 × (days - 7))`
- At 14 days: ~0.50, at 21 days: ~0.25, at 30 days: ~0.10

**Bioregional Delegation Tree:**

Attestation authority flows through a fractal hierarchy based on bioregions:

```
Temples of Refuge (Root — 1 worldwide)
    └── Realm Temples (Continental — 14)
        └── Subrealm Temples (Subcontinental — 52)
            └── Bioregion Temples (Regional — 185)
                └── Ecoregion Temples (Local — 844)
                    └── Individual Attesters (people on the land)
```

Each level delegates to the next via signed delegations. Trust in the chain is subjective.

**Proof of Life:**

Groups can record "Proof of Life" celebrations where all participants are attested simultaneously, refreshing their humanness.

---

## 5. Data Flow Diagrams

### Message Delivery (Offline Recipient)

```
Zephyr sends to Sage (Sage offline, Nova is mutual peer)

Zephyr                    Nova                      Sage
   │                        │                         │
   ├─ Route decision ───────┤                         │
   │  "Sage offline,        │                         │
   │   Nova is mutual"      │                         │
   │                        │                         │
   ├─── RELAY message ─────►│                         │
   │                        ├─ Store for Sage         │
   │                        │                         │
   │                        │       (time passes)     │
   │                        │                         │
   │                        │◄─── Sage comes online ──┤
   │                        │                         │
   │                        ├─── Deliver stored ─────►│
   │                        │                         │
   │◄── Back-prop ACK ──────┤◄──────── ACK ──────────┤
   │                        │                         │
```

### CRDT Document Sync

```
Peer A updates document          Peer B receives update

     ┌─────────────────┐              ┌─────────────────┐
     │  Local State    │              │  Local State    │
     │  { quests: [] } │              │  { quests: [] } │
     └────────┬────────┘              └────────┬────────┘
              │                                 │
     doc.update(|q| {                          │
       q.quests.push(...)                      │
     })                                        │
              │                                 │
     ┌────────▼────────┐                       │
     │  Serialize +    │                       │
     │  Broadcast      │──── gossip ──────────►│
     └────────┬────────┘                       │
              │                        ┌───────▼───────┐
     ┌────────▼────────┐              │  Deserialize  │
     │  Persist to     │              │  + Merge      │
     │  redb + events  │              └───────┬───────┘
     └─────────────────┘                      │
                                      ┌───────▼───────┐
                                      │  Rebuild      │
                                      │  Derived State│
                                      └───────────────┘
```

### Quest Workflow

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│   CREATE    │────►│   CLAIMED    │────►│  VERIFIED   │
│  (Creator)  │     │ (Claimants)  │     │  (Creator)  │
└─────────────┘     └──────────────┘     └──────┬──────┘
                                                │
                                         ┌──────▼──────┐
                                         │  COMPLETE   │
                                         │  (Creator)  │
                                         └─────────────┘

Details:
- CREATE: Quest posted with title, description, optional deadline
- CLAIMED: Multiple members submit claims with proof artifacts
- VERIFIED: Creator reviews and verifies valid claims (can verify multiple)
- COMPLETE: Creator marks quest done (no more claims accepted)
```

### Token Valuation (Subjective)

```
Token with steward chain: [Sage → Nova → Zephyr]
Blesser: Ember

Observer: Orion

Orion's sentiment:
  - Ember: +1 (trusted)
  - Sage: unknown
  - Nova: +0.5
  - Zephyr: unknown

Trust chain weight calculation:
  - Ember (blesser): 1.0 × 0.7^2 = 0.49  (2 hops to current)
  - Nova (chain[1]): 0.5 × 0.7^1 = 0.35  (1 hop to current)

  Best weight = max(0.49, 0.35) = 0.49

Humanness freshness (Ember): 0.8 (attested 10 days ago)

Final value = raw_attention × 0.49 × 0.8
            = 60,000ms × 0.49 × 0.8
            = 23,520ms subjective value to Orion
```

---

## 6. Key Design Decisions

### 1. Generic Identity Type

The `PeerIdentity` trait allows the same code to work with simulation identities (`char`) and production identities (`PublicKey`):

```rust
pub trait PeerIdentity: Clone + Eq + Hash + Display + Send + Sync + 'static {
    fn hash(&self, hasher: &mut impl Hasher);
}
```

This enables fast, deterministic simulation testing with human-readable peer names while production uses full cryptographic identities.

### 2. Sealed Packets

Relay nodes cannot read packet contents. The payload is encrypted end-to-end between source and destination. Relays only see:
- Packet ID
- Source/destination (for routing)
- TTL and visited set (for loop prevention)
- Priority (for queue ordering)

### 3. Deterministic ID Derivation

All system realm IDs are derived deterministically from member IDs:

```rust
// Any peer can compute Zephyr's inbox ID
let inbox = inbox_realm_id(zephyr_id);

// Both Zephyr and Nova compute the same DM realm ID
let dm = dm_realm_id(zephyr_id, nova_id);
assert_eq!(dm, dm_realm_id(nova_id, zephyr_id));
```

This enables:
- No discovery protocol for system realms
- Reproducible IDs across peers
- Offline computation of connection endpoints

### 4. Append-Only Event Logs

Events are never modified, only appended. This eliminates merge conflicts:

```rust
// Bad: mutable state causes conflicts
state.quest_count += 1;

// Good: append-only events with derived state
events.push(QuestCreated { ... });
// Derive count: events.iter().filter(|e| matches!(e, QuestCreated)).count()
```

### 5. Extension Traits for SDK

Domain logic extends the base SDK without modifying it:

```rust
// indras-sync-engine adds methods to Realm
pub trait RealmQuests {
    async fn create_quest(&self, ...) -> Result<QuestId>;
    async fn complete_quest(&self, id: QuestId) -> Result<()>;
}

impl RealmQuests for Realm { ... }
```

This keeps the SDK focused and allows clean separation between platform and application layers.

### 6. Subjective Trust Over Global Reputation

No global reputation scores exist. Each peer maintains their own:
- Contact list with sentiment
- Relayed sentiment from contacts
- Humanness assessments

A bad actor can't game "the system" because there is no system — only individual perspectives.

---

## 7. Technology Stack

| Component | Technology | Version | Notes |
|-----------|-----------|---------|-------|
| Language | Rust | 2024 Edition (1.87+) | Async/await, const generics |
| Runtime | Tokio | 1.47 | Multi-threaded async |
| P2P Transport | Iroh | 0.95 | QUIC-based, NAT traversal |
| CRDT | Automerge | 0.7 | JSON-like document sync |
| Crypto (PQ) | pqcrypto | - | ML-KEM-768, ML-DSA-65 |
| Symmetric Crypto | ChaCha20-Poly1305 | - | AEAD encryption |
| Hashing | BLAKE3 | - | Fast, parallel hashing |
| Serialization | Postcard | 1.0 | Compact binary format |
| Key-Value Store | redb | 2.4 | Embedded, ACID |
| UI Framework | Dioxus | 0.7 | React-like Rust UI |
| Async Channels | tokio::sync | - | mpsc, broadcast, RwLock |

---

## 8. Code Patterns & Examples

### Creating a Network and Realm

```rust
use indras_network::prelude::*;

#[tokio::main]
async fn main() -> Result<(), IndraError> {
    // Create network instance (one per device)
    let network = IndrasNetwork::new("~/.myapp").await?;

    // Create a realm for collaboration
    let realm = network.create_realm("Project Nexus").await?;

    // Share invite code with collaborators
    println!("Invite: {}", realm.invite_code().unwrap());

    Ok(())
}
```

### Joining a Realm via Invite

```rust
// Sage receives invite code from Nova
let invite = "indras://abc123...";
let realm = network.join_realm(invite).await?;

// Now Sage and Nova can collaborate
realm.send("Hello from Sage!").await?;
```

### Working with CRDT Documents

```rust
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Serialize, Deserialize)]
struct ProjectState {
    tasks: Vec<String>,
    notes: String,
}

// Get or create the document
let doc = realm.document::<ProjectState>("project").await?;

// Read current state
{
    let state = doc.read().await;
    println!("Tasks: {:?}", state.tasks);
}

// Update state (auto-synced to peers)
doc.update(|state| {
    state.tasks.push("Review design doc".to_string());
}).await?;

// Subscribe to changes from peers
let mut changes = doc.changes();
while let Some(change) = changes.next().await {
    if change.is_remote {
        println!("Update from {:?}", change.author);
    }
}
```

### Quest Workflow with SyncEngine

```rust
use indras_network::prelude::*;
use indras_sync_engine::prelude::*;

// Create the network
let network = Arc::new(IndrasNetwork::new("~/.myapp").await?);

// Create the sync engine app layer
let engine = SyncEngine::new(Arc::clone(&network));

// Create a quest in a realm
let realm = network.create_realm("Team Work").await?;
let quest_id = realm.create_quest(
    "Review architecture doc",
    "Please review and leave feedback on the attached PDF",
    None,  // No image
    my_id,
).await?;

// Another member claims the quest with proof
realm.submit_quest_claim(quest_id, nova_id, Some(proof_artifact)).await?;

// Creator verifies and completes
realm.verify_quest_claim(quest_id, 0).await?;  // Verify first claim
realm.complete_quest(quest_id).await?;
```

### Direct Peer Connection (DM)

```rust
// Zephyr wants to connect to Nova
// Computes deterministic DM realm ID
let dm_id = dm_realm_id(zephyr_id, nova_id);

// Send connection notification to Nova's inbox
let inbox_id = inbox_realm_id(nova_id);
let notify = ConnectionNotify::new(zephyr_id, dm_id)
    .with_name("Zephyr");

// Nova receives notification and joins the DM realm
// Both derive the same encryption key automatically
```

### Encounter Codes for In-Person Discovery

```rust
// At a conference, Zephyr generates a short-lived encounter code
let handle = network.start_encounter().await?;
println!("Share this code: {}", handle.code());

// Nova enters the code on their device
network.complete_encounter(&code).await?;

// Both are now mutual contacts with verified humanness
```

---

## 9. Glossary

| Term | Definition |
|------|------------|
| **Artifact** | Static, immutable content (file) shared within a realm. Content-addressed by hash. |
| **Attention** | Time spent working on a quest, tracked via focus-switch events. |
| **Back-Propagation** | ACK mechanism confirming message delivery through relay chain. |
| **Bioregional Delegation** | Hierarchical trust structure based on geographic bioregions for humanness attestation. |
| **Blessing** | Act of releasing accumulated attention as validation for a quest claim. |
| **CRDT** | Conflict-free Replicated Data Type. Enables automatic merging of concurrent edits. |
| **Direct Delivery** | Routing decision when destination is online and directly connected. |
| **DM Realm** | Deterministic realm for private messaging between exactly two peers. |
| **Document** | Typed, CRDT-backed data structure that auto-syncs across realm members. |
| **Encounter Code** | Short-lived code for in-person peer discovery and mutual attestation. |
| **Event Log** | Append-only log of interface events, used for store-and-forward. |
| **Freshness** | Measure of how recently a humanness attestation was recorded (decays over time). |
| **Gossip** | Peer discovery mechanism where peers share information about other peers. |
| **Hold** | Routing decision to store a packet for later delivery when destination comes online. |
| **HomeRealm** | Personal realm deterministically derived from MemberId, always exists. |
| **Humanness** | Proof-of-life attestation that someone is a real human, not a bot. |
| **Inbox** | System realm for receiving connection notifications from unknown peers. |
| **InterfaceId** | 32-byte identifier for a realm/interface. |
| **InviteCode** | Shareable code containing realm ID and encryption key for joining. |
| **Member** | A participant in a realm, identified by MemberId. |
| **MemberId** | 32-byte Ed25519 public key serving as both identity and network address. |
| **ML-DSA** | Module-Lattice Digital Signature Algorithm (Dilithium), post-quantum signatures. |
| **ML-KEM** | Module-Lattice Key Encapsulation Mechanism (Kyber), post-quantum key exchange. |
| **Mutual Peer** | A peer connected to both sender and recipient, useful as relay. |
| **Packet** | Store-and-forward message container with routing metadata. |
| **Peer-Set Realm** | Deterministic realm derived from sorted set of peer IDs. |
| **Post-Quantum** | Cryptographic algorithms resistant to quantum computing attacks. |
| **Proof Folder** | Collection of artifacts with narrative, submitted as quest proof. |
| **Proof of Service** | Model where quest claimants submit proof artifacts for verification. |
| **Quest** | Lightweight collaboration intention within a realm. |
| **QuestClaim** | Submission by a member claiming to have completed quest work. |
| **Realm** | Collaborative space where members share documents and artifacts. |
| **RealmId** | 32-byte unique identifier for a realm (alias for InterfaceId). |
| **Relay** | Routing decision to forward packet through mutual peer when not directly connected. |
| **Relayed Sentiment** | Second-degree sentiment from contacts' contacts, attenuated by 0.3×. |
| **Sentiment** | Subjective rating of another member: -1 (blocked), 0 (neutral), +1 (trusted). |
| **Steward Chain** | History of Token of Gratitude transfers from original recipient to current holder. |
| **Store-and-Forward** | Routing pattern where messages are stored at relay nodes for offline recipients. |
| **Subjective Value** | Token value that varies by observer based on their trust relationships. |
| **SyncEngine** | Application layer providing quests, blessings, tokens, attention, humanness. |
| **Token of Gratitude** | Transferable appreciation backed by blessed attention time. |
| **TTL** | Time-to-live: maximum hops before a packet is dropped (default: 10). |
| **Visited Set** | Hashes of peers who have handled a packet, prevents routing loops. |

---

## Document Metadata

- **Version**: 1.0
- **Last Updated**: 2026-02-07
- **Target Audience**: Claude Projects, AI assistants, new contributors
- **Word Count**: ~9,500

This knowledge base is designed for ingestion by Claude Projects to enable expert-level discussions about IndrasNetwork architecture, implementation patterns, and design philosophy.
