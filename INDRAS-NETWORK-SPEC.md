# Indra's Network — Core Data Model

## Overview

Indra's Network is the peer-to-peer network layer for SyncEngine, built on [iroh](https://iroh.computer). The entire system is built from three primitives: **one data structure** (Artifact), **one event** (Attention Switch), and **one relationship** (Mutual Peering).

Everything else — value, exchange, conversation, composition, discovery — emerges from these three.

---

## Primitives

### 1. Artifact

An Artifact is the only data structure in the system. It comes in two roles:

**Leaf Artifact** — immutable payload, no internal structure.
- Image, text, chat message, request, file, etc.
- Once placed in the Vault, the core payload never changes.

**Tree Artifact** — a mutable CRDT that organizes references to other Artifacts.
- Conversation (branching linked list of message artifacts)
- Gallery (spatial layout of image artifacts)
- Document (ordered collection of text/media artifacts)
- Any organizational structure that composes other artifacts

Tree Artifacts can reference other Tree Artifacts, producing fractal recursion.

**Every Artifact carries:**

| Field        | Type                | iroh Mapping | Mutability | Description                                                        |
|--------------|---------------------|--------------|------------|--------------------------------------------------------------------|
| `id`         | BLAKE3 hash (Leaf) or Doc ID (Tree) | Blob hash / Doc ID | Immutable  | Unique identifier                          |
| `payload`    | Bytes / Key-Value CRDT | Blob (Leaf) / Doc entries (Tree) | Leaf: immutable, Tree: mutable | The content or structure  |
| `steward`    | NodeID(s)           | Doc metadata key | Mutable    | Current authority over this artifact. Transferable.                 |
| `audience`   | List of NodeIDs     | Doc replica set  | Mutable    | Who can sync and attend to this artifact.                           |
| `references` | List of Artifact IDs | Doc entries (Tree only) | Mutable (Tree only) | Links to child artifacts in the structure.       |
| `created_at` | Timestamp           | Doc metadata key | Immutable  | When the artifact was placed in the Vault.                         |
| `type`       | Enum / Tag          | Doc metadata key | Immutable  | Leaf or Tree, plus content type hint (image, message, conversation, etc.) |

### 2. Attention Switch Event

The only event in the system. A Player moved their attention from one Artifact to another.

| Field        | Type          | Description                                    |
|--------------|---------------|------------------------------------------------|
| `player`     | Player ID     | Who switched attention                         |
| `from`       | Artifact ID   | Previous artifact (null if session start)       |
| `to`         | Artifact ID   | New artifact receiving attention               |
| `timestamp`  | Timestamp     | When the switch occurred                       |

Each Player produces their own **append-only log** of attention switch events as a personal iroh document. No conflicts are possible — your events are yours. Peers who have sync access to your log can incorporate it into their local attention computations.

### 3. Mutual Peering

A bidirectional trust relationship between two Players. Both parties must consent. This is the basis of the social graph and determines what each Player can see. Implemented as mutual sharing of attention log documents between iroh nodes.

| Field     | Type       | Description                     |
|-----------|------------|---------------------------------|
| `peer_a`  | NodeID     | First peer                      |
| `peer_b`  | NodeID     | Second peer                     |
| `since`   | Timestamp  | When the peering was established |

---

## Derived Computations

These are not stored globally. Each player computes them locally from their own merged state.

### Attention Value (per Artifact, per Player)

A Player's view of an artifact's attention value is computed from:
1. Collect all attention switch events referencing this artifact
2. Filter to events from the Player's **mutual peers** who are also in the artifact's **audience**
3. Aggregate (count of unique peers, total dwell time, recency-weighted, etc.)

There is **no global attention value.** Every Player sees a different number based on their peer graph. This is a feature, not a limitation — it produces a web-of-trust signal rather than a popularity metric.

### Dwell Time

Computed from consecutive attention switch events: `next_event.timestamp - this_event.timestamp` gives the time a player spent attending to an artifact.

### Activity / Heat

Recent attention switch density on an artifact from your peer network. A cluster of rapid switches from multiple peers = high activity.

---

## Core Operations

### Place an Artifact in the Vault
1. Create the Artifact: store payload as an iroh blob (Leaf) or create an iroh document (Tree).
2. Creator becomes initial Steward.
3. Audience is set by the Steward (replica set can start empty / private).
4. Artifact is now discoverable by anyone in the audience via document sync.

### Give Attention
1. Player opens an Artifact (must be in the audience / replica set).
2. An Attention Switch Event is appended to the Player's personal attention log document.
3. Mutual peers who sync this log can now incorporate it into their local attention computations.

### Compose (Tree Structure)
1. Create a Tree Artifact (new iroh document).
2. Add references to child Artifacts as document entries.
3. The tree structure is an iroh doc — concurrent edits (e.g., two people posting messages simultaneously) merge automatically via CRDT semantics.
4. Children do not need to know about their parent. The Tree holds all structural relationships.

### Exchange (Stewardship Transfer)
1. Player A tags their Artifact onto Player B's Artifact as an offer.
2. "Tagging" = creating a reference link between the two artifacts (likely via a Tree Artifact representing the exchange context).
3. If both parties agree, Stewardship of each Artifact transfers to the other party.
4. The Steward field on each Artifact is updated.
5. No separate transaction primitive is needed. Exchange = mutual stewardship transfer.

### Conversation
1. A Conversation is a Tree Artifact whose structure is a branching linked list.
2. Each message is a Leaf Artifact, stewarded by whoever sent it.
3. The Conversation Artifact maintains the tree shape — ordering, branching into threads.
4. The Conversation has its own Steward and Audience, independent of the message artifacts.
5. A message author can narrow their message's audience independently (retraction).
6. A gap appears in the tree where retracted messages were — this is honest.

### Audience Management
- Only the current Steward can modify an artifact's audience.
- Audience is explicit on every artifact — no inheritance.
- A steward can share a parent artifact widely while keeping child artifacts (e.g., negotiation chat) restricted.

---

## Implementation: Mapping to iroh Primitives

Indra's Network is built on top of [iroh](https://iroh.computer). The mapping from the abstract model to iroh's primitives is nearly direct.

### Player = iroh Node

Each Player is an iroh node identified by its **NodeID** (ed25519 public key). The NodeID is the Player ID throughout the system. QUIC connections with hole-punching via iroh's relay servers handle connectivity.

### Leaf Artifact = iroh Blob

An immutable Leaf Artifact (image, chat message, request, file) is stored as an **iroh blob** — content-addressed by its BLAKE3 hash. The blob hash *is* the artifact ID for leaves. Deduplication and integrity verification come for free. Blobs replicate on demand: a player fetches a blob by hash when they attend to the artifact.

### Tree Artifact = iroh Document

A mutable Tree Artifact (conversation, gallery, document) is an **iroh doc** — a mutable key-value CRDT that syncs across a defined set of replicas. The document holds:

- Metadata keys: `steward`, `type`, `created_at`
- Structure keys: references to child artifacts (blob hashes for leaves, document IDs for sub-trees)
- Ordering keys: sequence information for linked lists, spatial coordinates for galleries, etc.

Concurrent edits to a document merge automatically via iroh's CRDT semantics (per-author, last-writer-wins per key). Two people posting messages to a conversation simultaneously both succeed — the document merges both new references.

### Audience = Document Replica Set

The audience of an artifact is implemented as the **replica set of its iroh document.** When a Steward adds a player to the audience, they grant that player's NodeID sync access to the document. This means:

- Audience membership = sync permission on the iroh doc
- The artifact replicates across all audience members who choose to sync
- Removing a player from the audience revokes their sync access

For Leaf Artifacts (blobs), the audience is managed by a lightweight **metadata document** associated with the blob. This doc holds the steward, audience list, and attention data. The blob itself is content-addressed and can be fetched by anyone who knows the hash, but discovery of the hash is gated by the metadata doc's replica set.

### Attention Switch Logs = Per-Player iroh Documents

Each Player maintains their own **attention log document** — an iroh doc where only they write. Each entry is a key-value pair:

- Key: timestamp (or monotonic sequence number)
- Value: `{ from: artifact_id, to: artifact_id }`

This document is shared (read-only) with the player's mutual peers. Since only one author writes to it, there are no conflicts. Peers merge these logs locally to compute perspectival attention values.

### Mutual Peering = Bidirectional Document Sharing

A peering relationship is established when two players mutually share their attention log documents with each other. The act of granting sync access to your attention log *is* the act of peering. Both parties must reciprocate.

A player's **peer registry document** tracks their active peering relationships — a personal iroh doc listing NodeIDs of mutual peers and the document IDs of their shared attention logs.

### Communication Pattern

The fractal tree structure produces a natural **scoped lazy gossip** pattern using iroh's native sync:

1. **Tree spines sync eagerly.** iroh documents (Tree Artifacts) sync their key-value structure across the replica set continuously. When a new message reference is added to a conversation, all audience members learn of it quickly. These are lightweight — just keys and hashes.

2. **Leaf content pulls lazily.** Blob data (Leaf Artifacts) is fetched on demand when a player attends to the artifact. The attention switch event is the fetch trigger. iroh's blob fetching handles this — request by hash, pull from any peer who has it.

3. **Gossip is bounded by audience.** Sync only happens within each document's replica set. No broadcasting to the full mesh. Each artifact is its own scoped multicast group.

4. **Attention logs propagate through the peer graph.** Each player's attention log syncs only to mutual peers. Attention value computation is local — aggregate the logs you can see, filtered by the artifact's audience.

---

## Integrity & Mutual Accountability

Attention logs are append-only and synced to mutual peers. This creates a **mutual witnessing** protocol that ensures historical consistency without a global consensus mechanism.

**How it works:**

When a Player syncs their attention log to a mutual peer, that peer now holds a replica of the log. If the Player later attempts to rewrite their history — deleting events, altering timestamps, fabricating attendance — the peer's replica diverges from the modified version. The inconsistency is detectable by any peer holding a prior replica.

**Properties:**

- **Symmetric accountability.** Every peer relationship is bidirectional: you hold my history, I hold yours. Neither party can rewrite without the other noticing.
- **Social graph = verification layer.** The deeper your peer connections, the more witnesses hold your history, and the harder it is to falsify. Trust is proportional to connectivity.
- **Graceful trust degradation.** If a peer produces inconsistent logs, the divergence is visible. No governance mechanism is needed to adjudicate — the observing peer has the information and can choose to de-peer. The system doesn't punish dishonesty; it makes it visible.
- **No blockchain required.** Integrity comes from mutual replication across the peer graph, not from a global append-only ledger. Each peer validates the peers they care about.

**Implementation via iroh:** Since each player's attention log is an iroh document synced to mutual peers, iroh's document sync protocol provides the mechanism. Each replica tracks the document's history. A peer can detect if an author's entries have been modified or removed by comparing their synced state against incoming updates.

---

## Replication & Resilience

Because the audience maps directly to iroh's replica set, resilience is a natural consequence of sharing.

- The artifact survives as long as **any audience member** keeps their replica. iroh handles reconnection and re-sync when nodes come back online.
- If the Steward loses their device, the artifact and its full history persist across audience members. A new device re-peers and syncs back from the network.
- **Stewardship is social authority, not physical custody.** The Steward controls the replica set but does not have exclusive possession of the data.

**Resilience vs. Privacy tradeoff:** A narrow audience means fewer replicas and less resilience. A wide audience means high redundancy but broader exposure. The Steward implicitly chooses their redundancy level when they set the audience.

**Pinning is voluntary.** Audience members *may* keep their replica active but are not required to. A player can sync a document temporarily to view it, then stop syncing. Choosing to persist a replica is an act of care — helping preserve something for the network.

**No global ledger. No consensus.** iroh documents are CRDTs. Each peer merges what it has seen. Derived values (attention, heat, dwell time) are computed locally over merged state.

---

## Navigation Model

A Player moves through the fractal Artifact tree via Attention Switch Events. Their path through the tree is their experience of the system. The tree is navigable — you enter a Conversation Artifact and see its branching message tree, open a Gallery and see its spatial layout of images, follow a reference from one artifact to another.

Discovery happens through peer attention. If your mutual peers are attending to an artifact you haven't seen (and you're in its audience), that shows up as activity in your network.

---

## Summary of Invariants

1. **One primitive.** Everything is an Artifact.
2. **One event.** The only thing that happens is an Attention Switch.
3. **One relationship.** The only social structure is Mutual Peering.
4. **Stewardship is transferable.** Exchange = mutual stewardship transfer.
5. **Attention is perspectival.** No global value. Your view depends on your peers.
6. **Audience is explicit.** No inheritance. Each artifact controls its own access.
7. **Leaf payloads are immutable.** Once placed, the content doesn't change.
8. **Tree structures are mutable iroh documents.** They grow and branch as the system evolves, syncing across their replica set.
9. **No global ledger. No consensus.** iroh docs are CRDTs. Local computation over merged peer state.
10. **Audience = replica set.** Audience members sync artifacts, providing mutual backup. Stewardship is social authority, not physical custody.
11. **Mutual witnessing.** Peers hold each other's attention history. Inconsistency is detectable. The social graph is the verification layer.
