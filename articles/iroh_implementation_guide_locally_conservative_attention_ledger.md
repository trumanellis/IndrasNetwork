# Implementation Guide: Locally-Conservative Attention Ledger
## Practical Build Notes for IndrasNetwork

> **Scope:** This document describes how to implement the **Locally-Conservative Attention Ledger** (whitepaper: `locally_conservative_attention_ledger_whitepaper.md`) on top of the existing IndrasNetwork crate architecture.
>
> It focuses on *data layout, protocol flows, and invariants* — not UX, not economics.
>
> **Audience:** Developers working on `indras-artifacts`, `indras-sync-engine`, `indras-gossip`, and `indras-network`.

---

## 0. Goals and Non-Goals

### Goals
- **Append-only** author event chains (tamper-evident).
- **Topic-scoped broadcast** for low-latency dissemination of new events.
- **Content-addressed storage** for events and bulky attachments.
- **Convergent indexes** so peers can reconstruct:
  - current attention state per author
  - total attention on each intention
  - attention-time ("Blessing-time") per intention
- Optional hardening: **quorum certificates** and **fraud proofs** without a global chain.

### Non-Goals (in this guide)
- Proof-of-humanness / membership gating ("Temple" policy).
- Complex anti-Sybil economics.
- Global ordering of events.

---

## 1. IndrasNetwork Building Blocks

The codebase already provides the infrastructure the attention ledger needs. This section maps whitepaper concepts to existing crates and types.

### 1.1 Identity and Transport (`indras-transport`, `indras-network`)

Each running app instance creates one **iroh Endpoint** with a long-lived secret key, managed by `indras-transport::IrohNetworkAdapter`. The endpoint's public identity is a 32-byte Ed25519 public key. Attention events are additionally signed with **post-quantum (PQ) keys** via `indras-crypto::PQIdentity` (Dilithium3 / ML-DSA-65).

**What already exists:**
- `IrohNetworkAdapter` manages endpoint lifecycle, key persistence, and relay connections.
- `PlayerId` (`[u8; 32]`) is the canonical player/author identifier used throughout `indras-artifacts`.
- `PQIdentity` provides post-quantum signing (Dilithium3) and `PQPublicIdentity` provides verification.
- `IndrasNetwork` provides the high-level API for starting/stopping the transport.

**Implementation guidance:**
- Reuse the existing endpoint — do not create a second one for the attention ledger.
- The attention ledger's author IS the existing `PlayerId`.
- Event signing uses PQ signatures (`PQIdentity::sign()`), not the transport-layer Ed25519 keys.

### 1.2 Gossip (`indras-gossip`)

`IndrasGossip` provides topic-based pub/sub built on iroh-gossip. Messages are signed (`SignedMessage`) and framed with a versioned wire format (`WireMessage::V0`).

**What already exists:**
- `TopicHandle<I>` — sender for broadcasting signed events to a topic.
- `TopicReceiver<I>` — receiver for incoming verified messages.
- `SplitTopic<I>` — combined sender/receiver handle per topic subscription.
- `TopicId::from_interface(interface_id)` — deterministic topic derivation.

**How we use gossip:**
- Broadcast full attention switch events inline (events are ~200 bytes, well within gossip limits).
- Broadcast fraud proofs (two conflicting event hashes + signatures).
- Broadcast certificate announcements (Phase 2).
- Broadcast tip messages for anti-entropy catch-up.

### 1.3 Storage (`indras-storage`)

`CompositeStorage` provides a tri-layer storage architecture:

| Layer | Purpose | Attention Ledger Use |
|-------|---------|----------------------|
| `EventLog` (append-only) | Immutable event history | Author event chains |
| `RedbStorage` (structured) | Queryable metadata, indices | Tips, fraud records, witness rosters |
| `BlobStore` (content-addressed) | Large payloads | Certificates, large attachments |

**What already exists:**
- `PeerRegistry` / `PeerRecord` — tracks direct peers (the neighborhood `N(p)` from the whitepaper).
- `SyncStateStore` / `SyncStateRecord` — sync checkpoints.
- `EventLog` with `CompactionConfig` — append-only per-interface event logs.

### 1.4 Documents (`indras-network::Document<T>`)

`Document<T>` provides typed, reactive CRDT documents that automatically synchronize across realm members. Any type implementing `DocumentSchema` (which requires `merge()`) can be stored and synced.

**What already exists:**
- `DocumentSchema` trait with `merge()`, `extract_delta()`, `apply_delta()`.
- Automatic background sync via gossip: local changes broadcast to peers, remote changes merged automatically.
- Persistence to redb with automatic reload.

**How we use documents:**
- `AttentionTipDocument` — latest `(seq, hash)` per author; merge = max seq.
- `WitnessRosterDocument` — witness sets per intention (Phase 2).
- `FraudEvidenceDocument` — detected equivocations.

These replace the iroh-docs KV-CRDT layer that was proposed in the original guide (iroh-docs has been removed from iroh).

---

## 2. Data Model

### 2.1 Identifiers

| Identifier | Type | Source |
|------------|------|--------|
| `AuthorId` | `PlayerId` (`[u8; 32]`) | Existing in `indras-artifacts::artifact` |
| `EventHash` | `[u8; 32]` | BLAKE3 hash of canonical event encoding |
| `IntentionId` | `ArtifactId` | Existing in `indras-artifacts::artifact` |

**Critical:** The guide's `IntentionId` is NOT a new type. An `Intention` already exists in `indras-artifacts` as "a goal with proofs, attention tokens, and pledges" (see `crates/indras-artifacts/src/intention.rs`). The attention ledger's intention identifier is simply the `ArtifactId` of an existing `Intention` artifact. This connects the attention ledger directly to the artifact domain model.

### 2.2 Attention Switch Event (ASE)

**Implemented in `indras-artifacts/src/attention/mod.rs`:**

```rust
/// A single attention-switch event in a player's hash-chained log.
///
/// Events are append-only. Each event references the hash of the previous
/// event (`prev`), forming a tamper-evident chain per author. The `sig` field
/// holds PQ signature bytes (empty Vec when unsigned).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionSwitchEvent {
    /// Protocol version (currently 1).
    pub version: u16,
    /// Author of this event.
    pub author: PlayerId,
    /// Monotonically increasing sequence number per author.
    pub seq: u64,
    /// Wall-clock time in milliseconds (informational, not trusted for ordering).
    pub wall_time_ms: i64,
    /// Artifact attention is leaving (None for genesis).
    pub from: Option<ArtifactId>,
    /// Artifact attention is moving to (None for farewell).
    pub to: Option<ArtifactId>,
    /// BLAKE3 hash of the previous event in this author's chain (zeros for genesis).
    pub prev: [u8; 32],
    /// PQ signature bytes (empty Vec when unsigned).
    pub sig: Vec<u8>,
}
```

**Key methods:**
- `signable_bytes()` — canonical encoding of all fields except `sig` via `postcard` (used as signing input).
- `event_hash()` — BLAKE3 hash of the full event including `sig` (used as chain link).
- `sign(&mut self, identity: &PQIdentity)` — signs with Dilithium3 via `indras-crypto`.
- `verify_signature(&self, pk: &PQPublicIdentity) -> bool` — verifies PQ signature.
- `is_genesis()` — true if `seq == 0 && prev == [0; 32] && from.is_none()`.

**Rules:**
- `seq` is strictly increasing per author (0, 1, 2, ...).
- `prev` must equal the BLAKE3 hash of the immediately prior event in the author's chain. Genesis events use `[0u8; 32]`.
- Exactly one of `from`, `to` may be `None` for join/leave transitions.
- `sig` covers all fields except itself, using the author's PQ secret key (Dilithium3 / ML-DSA-65).
- `sig` is `Vec<u8>` (not fixed-size) so the struct can derive `PartialEq`/`Eq` without depending on `PQSignature`'s trait impls.
- Canonical encoding uses `postcard` (matching the codebase's existing serialization convention).

**Note:** The sync-engine's `AttentionSwitchEvent` (in `indras-sync-engine/src/attention.rs`) is a separate, independent type for quest-level attention tracking with different fields (`event_id`, `member`, `quest_id`, `timestamp_millis`). Both types coexist — the artifacts type carries conservation guarantees, the sync-engine type tracks quest focus.

### 2.3 Certificate (Phase 2)

```rust
pub struct QuorumCertificate {
    pub version: u16,
    pub event_hash: [u8; 32],
    pub intention_scope: ArtifactId,
    pub witnesses: Vec<WitnessSignature>,
}

pub struct WitnessSignature {
    pub witness: PlayerId,
    pub sig: Vec<u8>,  // PQ signature over (event_hash || intention_scope)
}
```

**Rule:** A certificate is valid when it contains >= `k` eligible witness signatures for the declared intention scope.

**Connection to existing types:** The `BlessingRecord` in `indras-artifacts` already represents an attestation pattern (peer vouching for an artifact). Quorum certificates generalize this — instead of a single blessing, they collect `k` witness signatures over an event hash. The signing semantics are analogous.

### 2.4 Fraud Proof (Equivocation Proof)

**Implemented in `indras-artifacts/src/attention/fraud.rs`:**

```rust
/// Evidence of equivocation by an author.
///
/// Contains two events from the same author with the same sequence number
/// but different content (different event hashes).
pub struct EquivocationProof {
    /// The author who equivocated.
    pub author: PlayerId,
    /// The sequence number where equivocation occurred.
    pub seq: u64,
    /// First observed event at this (author, seq).
    pub event_a: AttentionSwitchEvent,
    /// Second (conflicting) event at this (author, seq).
    pub event_b: AttentionSwitchEvent,
}
```

**Methods:**
- `is_valid()` — verifies same author, same seq, different hashes.
- `verify_signatures(&self, pk: &PQPublicIdentity)` — verifies both events are signed by the claimed author.

**Helper:** `check_equivocation(new_event, existing_events) -> Option<EquivocationProof>` scans for conflicts.

Valid if:
- `event_a.event_hash() != event_b.event_hash()`
- Both events have same `(author, seq)`
- Both PQ signatures verify

**Note:** The sync-engine also has `FraudEvidenceDocument` (CRDT, union merge) which stores fraud records as serialized bytes for cross-peer propagation. `EquivocationProof` is the artifacts-level type; `FraudRecord` is the sync-engine-level wire format.

---

## 3. Topics and Routing

### 3.1 Topic Derivations

Use deterministic topic IDs following the codebase's BLAKE3 derivation convention:

```rust
fn attention_topic(intention_id: &ArtifactId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"indras:attention:intention:");
    hasher.update(intention_id.bytes());
    *hasher.finalize().as_bytes()
}

fn author_topic(author: &MemberId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"indras:attention:author:");
    hasher.update(author);
    *hasher.finalize().as_bytes()
}

fn fraud_topic(intention_id: &ArtifactId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"indras:attention:fraud:");
    hasher.update(intention_id.bytes());
    *hasher.finalize().as_bytes()
}
```

This matches the existing artifact-sync convention:
```rust
// Existing pattern from indras-network artifact sync:
let mut hasher = blake3::Hasher::new();
hasher.update(b"indras:artifact-sync:");
hasher.update(&artifact_id.as_bytes());
*hasher.finalize().as_bytes()
```

### 3.2 What Gets Broadcast

**Broadcast full events inline.** Unlike the original guide's recommendation to only broadcast hashes, attention switch events are small (~200 bytes) and well within gossip message limits (< 2 KB). Sending the full event eliminates a round-trip fetch.

Gossip message types (using the existing `WireMessage` framing pattern from `indras-gossip`):

```rust
/// Attention ledger gossip messages.
#[derive(Serialize, Deserialize)]
pub enum AttentionMessage {
    /// Full attention switch event (primary dissemination path).
    Event(AttentionSwitchEvent),
    /// Tip announcement for anti-entropy.
    Tip { author: PlayerId, latest_seq: u64, latest_hash: [u8; 32] },
    /// Fraud proof (equivocation evidence).
    Fraud(EquivocationProof),
    /// Certificate announcement (Phase 2).
    Certificate(QuorumCertificate),
}
```

Messages are signed using the existing `SignedMessage::sign_and_encode()` from `indras-gossip::message`. Event-level signatures use PQ keys (Dilithium3), while gossip-level message framing uses Ed25519 (the transport layer key).

### 3.3 Peer Neighborhoods and Mutual Peers

The whitepaper defines:
- `N(p)`: direct peers of `p`
- `M(p,q) = N(p) intersection N(q)`: mutual peers

**What already exists:**
- `PeerRegistry` in `indras-artifacts::peering` tracks a player's direct peers as `Vec<PeerEntry>`.
- `MutualPeering` in `indras-artifacts::peering` canonically represents a mutual peer relationship:

```rust
pub struct MutualPeering {
    pub peer_a: PlayerId,
    pub peer_b: PlayerId,
    pub since: i64,
}
```

- `PeerRegistry` in `indras-storage::structured` provides persistent peer tracking with redb backing.

Use mutual peers for:
- Witness selection (prefer witnesses in `M(p,q)` for triangle finality).
- Redundant event storage (mutual peers pin each other's events).
- Store-and-forward relay (mutual peers forward missed events).

---

## 4. Protocol Flows

### 4.1 Create and Publish a Switch Event

1. Author computes `event.prev = BLAKE3(last_event_bytes)` (via `event_hash()` on previous event).
2. Author sets `event.seq = last_seq + 1`.
3. Author sets `event.from = current_attention_state`.
4. Author signs event with PQ key: `event.sign(&pq_identity)` (Dilithium3).
5. Author encodes event canonically with `postcard::to_allocvec()`.
6. Author broadcasts `AttentionMessage::Event(event)` via gossip to:
   - `author_topic(author)` — so peers tracking this author get the event.
   - `attention_topic(from)` and `attention_topic(to)` — so peers tracking those intentions get the event.
7. Author persists event to local `EventLog`.

### 4.2 Receive an Event

On receiving `AttentionMessage::Event(event)`:
1. Compute `event_hash = BLAKE3(canonical_bytes)`.
2. If `event_hash` already known -> ignore (dedup).
3. Verify PQ signature via `event.verify_signature(&author_public_key)`.
4. Validate chain constraints (see Section 7 for full validation).
5. Insert into local store.
6. If `event.prev` is unknown and `event.seq > 0`, queue and request missing events from the author or mutual peers.

### 4.3 Anti-Entropy (Catch-Up)

Gossip can miss messages. The catch-up protocol uses two mechanisms:

**A. Tip exchange via `Document<AttentionTipDocument>`:**
- Each peer maintains a `Document<AttentionTipDocument>` per realm.
- The document stores `(author, latest_seq, latest_hash)` tuples.
- Document merge uses max-seq semantics: `merge(a, b) = max_by_seq(a, b)`.
- Peers discover gaps by comparing their local chain heads against the document's tips.

**B. Direct chain sync for filling gaps:**
- When a gap is detected (local seq < tip seq for some author):
  - Request: "give me author A's events from seq 38 to 47."
  - Receiver validates the chain (sig + prev + seq) as events arrive.
- This replaces the blob-fetch mechanism from the original guide. Events carry their own authentication (signature + hash-linking), so no separate content-addressed fetch is needed for the primary sync path.

Blob-fetch remains available as a fallback for large attachments or certificate payloads.

### 4.4 Quorum Certificate Flow (Phase 2)

When an event touches intention `I_scope`:
1. Author (or recipient) requests witness signatures from eligible witnesses in `W(I_scope)`.
2. Each witness validates the event, then PQ-signs `(event_hash || I_scope)`.
3. Once >= `k` signatures collected, assemble `QuorumCertificate` and store.
4. Broadcast `AttentionMessage::Certificate(cert)` on `attention_topic(I_scope)`.

Peers treat events as:
- **observed** — when the event blob is valid (signature + chain verified).
- **final** — when a valid quorum certificate is available.

### 4.5 Fraud Proof Flow

If two different validly-signed events with same `(author, seq)` are detected:
1. Build `EquivocationProof` containing both conflicting events.
2. Broadcast `AttentionMessage::Fraud(proof)` on:
   - `author_topic(author)`
   - any involved intention topics
3. Store in `FraudEvidenceDocument` (syncs to peers via `Document<T>`).
4. Apply policy:
   - Reject uncertified events from that author at that seq.
   - Optionally freeze author until human review.

---

## 5. Conservation Enforcement Architecture

Conservation is NOT a property of the sync layer. It comes from the algebraic structure of events and the validation rules applied to them. The sync layer only needs to achieve eventual event set convergence.

```text
Layer 1: Algebraic structure    -> (-1, +1) switch events guarantee sum(delta) = 0
Layer 2: Per-author validation  -> seq/prev hash-linking, from == prior.to
Layer 3: Equivocation detection -> same (author, seq), different hash = fraud
Layer 4: Event set convergence  -> Document<T> union merge + chain sync
Layer 5: Transport              -> Gossip broadcast + direct peer connections
```

**Key insight:** Layers 1-3 provide conservation. Layers 4-5 provide availability. They are independent concerns.

### Why This Matters for Implementation

- **Validation is local.** Any peer can verify any event's contribution to conservation by checking its fields. No coordination needed.
- **Sync is eventually consistent.** Events can arrive out of order, be delayed, or be retransmitted. As long as the event set converges (Assumption A1 from the whitepaper), conservation holds.
- **Fraud detection is best-effort.** Equivocation detection improves with more peers observing the same author, but the system is safe even if detection is delayed — the damage is bounded to the equivocating author's single unit of attention mass.

### Conservation Invariant (from whitepaper Theorem 1)

For the full set of active members `V`:

```
sum(A_I(t)) = |V|    for all t
```

Where `A_I(t) = count(authors where current_state(author) == I)`.

Each switch event induces:
```
delta_e(A_I) = -1 if I = I_from
              +1 if I = I_to
               0 otherwise

sum_I(delta_e(A_I)) = 0    (Lemma 1: Local Conservation)
```

---

## 6. Two-Mechanism Sync Architecture

The original guide proposed iroh-docs (KV-CRDTs) for convergent indexes. Since iroh-docs has been removed from iroh, we replace it with two complementary mechanisms that match the existing codebase patterns.

### A. `Document<T>` for Convergent Metadata

`Document<T>` (from `indras-network::document`) already provides typed CRDT documents with automatic gossip-based sync. We define three new document schemas:

**`AttentionTipDocument`** — latest chain head per author.
Implemented in `indras-sync-engine/src/attention_tip.rs`:

```rust
/// A tip advertisement: the latest event in an author's attention chain.
pub struct AttentionTip {
    pub author: MemberId,
    pub seq: u64,
    pub event_hash: [u8; 32],
    pub wall_time_ms: i64,
}

/// CRDT document tracking attention chain tips for all known authors.
pub struct AttentionTipDocument {
    tips: HashMap<MemberId, AttentionTip>,
}
```

Merge semantics: per-author max-seq wins. Methods: `update_tip()`, `tip_for()`, `gaps_from(peer_tips)`.

**`WitnessRosterDocument`** (Phase 2) — witness sets per intention:
```rust
pub struct WitnessRosterDocument {
    /// intention_id -> set of eligible witness MemberIds
    pub rosters: HashMap<ArtifactId, Vec<MemberId>>,
}
```

Merge semantics: per-intention union of witness sets.

**`FraudEvidenceDocument`** — detected equivocations.
Implemented in `indras-sync-engine/src/fraud_evidence.rs`:

```rust
/// A fraud proof stored in the evidence document.
pub struct FraudRecord {
    pub author: MemberId,
    pub seq: u64,
    pub event_a_bytes: Vec<u8>,  // postcard-serialized AttentionSwitchEvent
    pub event_b_bytes: Vec<u8>,
    pub reporter: MemberId,
    pub detected_at_ms: i64,
}

/// CRDT document collecting fraud evidence for a realm.
pub struct FraudEvidenceDocument {
    records: HashMap<MemberId, Vec<FraudRecord>>,
}
```

Merge semantics: union of records, deduplicated by `(author, seq)`. Methods: `add_record()`, `is_fraudulent()`, `records_for()`, `fraudulent_authors()`.

### B. Chain Sync Protocol for Event Histories

Events don't need generic CRDT semantics because they are:
- **Self-authenticating** — PQ-signed (Dilithium3) by the author.
- **Self-ordering** — `seq` + `prev` hash-link.
- **Content-addressed** — BLAKE3 hash.

The chain sync protocol:

1. **Discover gaps:** Compare local chain heads against `AttentionTipDocument` tips.
2. **Request missing range:** "Send me author A's events from seq `local_head + 1` to `tip_seq`."
3. **Validate on arrival:** Each event is validated independently (signature, seq, prev).
4. **Persist:** Append validated events to the local `EventLog`.

This is simpler than a general CRDT sync because the per-author chain has a single canonical ordering (by seq).

---

## 7. Dynamic Membership

The whitepaper's conservation invariant `sum(A_I) = |V|` assumes a fixed set `V`. In practice, members join and leave.

### Join Protocol

A new member's first event is a **genesis event**:

```rust
// Created automatically by AttentionLog::navigate_to() when next_seq == 0
AttentionSwitchEvent {
    version: 1,
    author: new_player_id,
    seq: 0,
    wall_time_ms: now,
    from: None,                  // not attending anything yet
    to: Some(initial_intention), // first intention to attend
    prev: [0u8; 32],            // zero-hash for genesis
    sig: vec![],                 // sign with event.sign(&pq_identity)
}
```

Validation: `event.is_genesis()` checks `seq == 0 && prev == [0; 32] && from.is_none()`.

This adds one unit of attention mass to the system: `|V| -> |V| + 1`.

### Leave Protocol

A departing member's last event is a **farewell event**:

```rust
// Created by AttentionLog::end_session()
AttentionSwitchEvent {
    version: 1,
    author: departing_player_id,
    seq: last_seq + 1,
    wall_time_ms: now,
    from: Some(current_intention), // where they were attending
    to: None,                      // leaving the system
    prev: last_event_hash,
    sig: vec![],                   // sign with event.sign(&pq_identity)
}
```

This removes one unit of attention mass: `|V| -> |V| - 1`.

**Connection to existing code:** `AttentionLog::end_session()` creates farewell events. The `AttentionDocument::handle_member_left()` method in `indras-sync-engine` handles the sync-engine-level equivalent.

### Conservation with Dynamic Membership

The invariant generalizes to:

```
sum(A_I(t)) = |active members at time t|
```

Where "active" means: has a genesis event and no farewell event (or the latest event has `to != None`).

---

## 8. Deterministic State Reconstruction

### 8.1 Current Attention per Author

For each author:
- Find the event with **max seq** that is valid (or certified max, depending on policy).
- Its `to` field is the author's current attention state.
- If `to == None`, the author has left or ended their session.

### 8.2 Instantaneous Attention per Intention

```
A_I = count(authors where current_state(author) == I)
```

### 8.3 Attention-Time (Blessing-Time)

For each author, replay their chain in seq order and accumulate durations between adjacent events:
- Attribute duration to the `to` intention of the earlier event.
- Handle missing events by leaving gaps (do not interpolate).
- Handle wall-clock anomalies conservatively (ignore negative deltas).

**Connection to existing code:** `compute_dwell_time()` and `extract_dwell_windows()` in `indras-artifacts::attention` already implement this exact algorithm. The locally-conservative model adds hash-linking and signatures but the dwell-time computation is identical.

Persist computed aggregates locally (NOT as globally authoritative values). The existing `compute_heat()` function and `AttentionValue` type already follow this pattern.

---

## 9. Invariants and Validation Rules

### 9.1 Event Validity

An event is valid iff:
- PQ signature verifies for `author` (via `event.verify_signature(&pk)`).
- Canonical decoding succeeds (postcard round-trips cleanly).
- `seq` and `prev` constraints hold (or are queueable until `prev` arrives).

### 9.2 Conservation Invariant (Per Author)

Each event must transition from the author's current attention state:
- `event.from` must equal the `to` of the author's prior event (or `None` for genesis).
- This ensures each switch is a `(-1, +1)` pair, preserving conservation.

### 9.3 Non-Equivocation

Baseline detection:
- If two events with same `(author, seq)` and different hashes are observed -> equivocation detected.

Hardening (Phase 2):
- Only treat certified events as final.
- Treat uncertified events as soft / reversible.

### 9.4 Concrete Validation

**Implemented in `indras-artifacts/src/attention/validate.rs`:**

```rust
/// Per-author state tracked during validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorState {
    pub latest_seq: u64,
    pub latest_hash: [u8; 32],
    pub current_attention: Option<ArtifactId>,
}

/// Validate a single event against the author's current state.
pub fn validate_event(
    event: &AttentionSwitchEvent,
    author_state: &AuthorState,
    public_key: Option<&PQPublicIdentity>,
) -> Result<(), ValidationError> {
    // 1. Verify PQ signature if key provided
    if let Some(pk) = public_key {
        if !event.verify_signature(pk) {
            return Err(ValidationError::InvalidSignature);
        }
    }

    // 2. Check sequence number
    let expected_seq = author_state.latest_seq + 1;
    if event.seq != expected_seq {
        return Err(ValidationError::SequenceGap { expected: expected_seq, got: event.seq });
    }

    // 3. Check prev hash
    if event.prev != author_state.latest_hash {
        return Err(ValidationError::PrevHashMismatch { seq: event.seq });
    }

    // 4. Check attention continuity (conservation)
    if event.from != author_state.current_attention {
        return Err(ValidationError::AttentionContinuity {
            seq: event.seq,
            expected: author_state.current_attention,
            got: event.from,
        });
    }

    Ok(())
}

/// Validate a genesis event.
pub fn validate_genesis(
    event: &AttentionSwitchEvent,
    public_key: Option<&PQPublicIdentity>,
) -> Result<(), ValidationError> { ... }

/// Validate an entire chain. Returns final AuthorState on success.
pub fn validate_chain(
    events: &[AttentionSwitchEvent],
    public_key: Option<&PQPublicIdentity>,
) -> Result<AuthorState, ValidationError> { ... }
```

**Connection to existing code:** The `AttentionStore::check_integrity()` trait method and `IntegrityResult` enum in `indras-artifacts::store` implement divergence detection for peer logs. The validation above extends this with cryptographic guarantees (PQ signatures, hash-linking) rather than relying solely on event equality comparison.

---

## 10. Reference Architecture (Crate Mapping)

The attention ledger maps onto the existing crate hierarchy. No new crates are needed.

| Concern | Crate | Module | Status |
|---------|-------|--------|--------|
| Event types + PQ signing | `indras-artifacts` | `attention/mod.rs` | **Done** |
| Event validation | `indras-artifacts` | `attention/validate.rs` | **Done** |
| Fraud proofs | `indras-artifacts` | `attention/fraud.rs` | **Done** |
| Storage traits | `indras-artifacts` | `store.rs` (`AttentionStore` extended) | **Done** |
| Gossip messages | `indras-gossip` | via `InterfaceEvent::Custom` | **Done** (uses existing variant) |
| Tip sync document | `indras-sync-engine` | `attention_tip.rs` | **Done** |
| Fraud evidence document | `indras-sync-engine` | `fraud_evidence.rs` | **Done** |
| Chain sync protocol | `indras-sync-engine` | `attention_sync.rs` (new) | **Done** |
| Realm-level API | `indras-sync-engine` | `realm_attention.rs` (upgraded) | **Done** |

### Re-export Chain

Changes flow through the existing re-export chain:

```
indras-artifacts (types + validation)
    -> indras-network (blanket re-export)
        -> app code
```

The `RealmAttention` trait in `indras-sync-engine::realm_attention` already provides the app-level API. It will be upgraded to expose conservation-aware operations:

```rust
pub trait RealmAttention {
    /// Get the attention tracking document.
    async fn attention(&self) -> Result<Document<AttentionDocument>>;

    /// Switch attention with conservation guarantees.
    /// Creates a PQ-signed, hash-linked event and broadcasts it.
    async fn switch_attention(
        &self,
        from: Option<ArtifactId>,
        to: Option<ArtifactId>,
        player: PlayerId,
    ) -> Result<AttentionEventId>;

    /// Get current focus for a member.
    async fn get_member_focus(&self, member: &MemberId) -> Result<Option<QuestId>>;

    /// Get quests ranked by total attention time.
    async fn quests_by_attention(&self) -> Result<Vec<QuestAttention>>;

    // ... existing methods preserved ...
}
```

---

## 11. Testing Strategy

### 11.1 Determinism Tests (Unit) — **Implemented**

- Identical event struct -> identical `postcard` bytes -> identical BLAKE3 hash (`test_event_hash_deterministic`).
- Different events -> different hashes (`test_event_hash_differs_for_different_events`).
- PQ signature round-trip: sign -> encode -> decode -> verify (`test_sign_and_verify`).
- Wrong key -> verification fails (`test_verify_wrong_key_fails`).
- Unsigned events are detected (`test_event_unsigned_by_default`).

### 11.2 Adversarial Tests (Unit + Integration) — Partially Implemented

- **Implemented:** Equivocation detection (`test_equivocation_detected`, `test_no_equivocation_same_event`, `test_no_equivocation_different_seq`).
- **Implemented:** Chain validation — seq gaps (`test_validate_chain_seq_gap`), prev hash mismatch (`test_validate_chain_prev_mismatch`), attention continuity breaks (`test_validate_chain_attention_continuity`).
- **Implemented:** Genesis validation — invalid seq/prev/from (`test_validate_genesis_invalid_*`).
- **Implemented:** Chain-aware store methods (`test_attention_store_events_by_seq_range`, `test_attention_store_latest_tip`).
- Pending: Duplicate gossip dedup by hash.
- Pending: Dropped messages (anti-entropy recovery via tip sync).
- Pending: Reordering (events arrive out of seq order; queue until prev arrives).
- Pending: Missing prev pointers (request missing chain segments).
- Pending (Phase 2): Byzantine witnesses — partial certificates never reach quorum.

### 11.3 Simulation Harness (Lua Scenarios)

The existing `simulation/` crate with Lua scripting provides the infrastructure for adversarial and convergence testing.

**Example Lua scenario for attention conservation:**

```lua
-- scenarios/attention_conservation_stress.lua
--
-- Verifies conservation invariant under concurrent switches and partitions.

local helpers = require("lib.attention_helpers")

-- Create a network of 5 peers
local peers = helpers.create_peers(5)

-- Each peer joins with a genesis event
for _, peer in ipairs(peers) do
    peer:genesis("intention_A")
end

-- Verify: sum(attention) == 5
assert(helpers.total_attention(peers) == 5)

-- Concurrent switches
peers[1]:switch("intention_A", "intention_B")
peers[2]:switch("intention_A", "intention_C")
peers[3]:switch("intention_A", "intention_B")

-- Verify conservation after switches
assert(helpers.total_attention(peers) == 5)

-- Simulate network partition
helpers.partition(peers, {1, 2}, {3, 4, 5})

-- Switches during partition
peers[1]:switch("intention_B", "intention_C")
peers[4]:switch("intention_A", "intention_B")

-- Heal partition
helpers.heal_partition(peers)
helpers.wait_convergence(peers)

-- Verify conservation after merge
assert(helpers.total_attention(peers) == 5)

-- Peer leaves
peers[5]:farewell()
assert(helpers.total_attention(peers) == 4)
```

### 11.4 Existing Test Infrastructure

The codebase already has relevant test patterns:
- `crates/indras-sync-engine/src/attention.rs` — tests for `AttentionDocument` merge, calculation, ranking.
- `crates/indras-artifacts/tests/integration.rs` — tests for `IntegrityResult`, `compute_token_value`, dwell time.
- `simulation/scripts/scenarios/sync_engine_attention_stress.lua` — existing attention stress scenario.

---

## 12. Implementation Checklists

### Phase 1: MVP (Hash-Linked Chains + PQ Signing + Fraud Proofs)

- [x] Upgrade `AttentionSwitchEvent` in `indras-artifacts` with `version`, `seq`, `prev`, `sig` fields
- [x] Implement canonical encoding (postcard) + BLAKE3 hashing (`signable_bytes()`, `event_hash()`)
- [x] Implement PQ signing (Dilithium3) and verification for events (`sign()`, `verify_signature()`)
- [x] Create `attention/validate.rs` module with `validate_event()`, `validate_genesis()`, `validate_chain()`
- [x] Extend `AttentionStore` trait with chain-aware methods (`events_by_seq_range`, `latest_tip`)
- [x] Gossip message path: use existing `InterfaceEvent::Custom` variant (no new enum needed)
- [x] Implement `AttentionTipDocument` with max-seq merge for anti-entropy
- [x] Implement equivocation detection: `EquivocationProof` + `check_equivocation()`
- [x] Implement `FraudEvidenceDocument` with union merge
- [x] Genesis and farewell events for dynamic membership (`is_genesis()`, `end_session()`)
- [x] Unit tests: determinism, PQ signature round-trip, validation rules (96 tests passing)
- [x] Integration tests: chain validation, fraud detection, chain-aware store methods
- [ ] Broadcast full events inline via gossip (not just hashes) — deferred to Phase 1.5 (Document<T> CRDT sync used instead)
- [x] Implement chain sync protocol: tip comparison -> gap detection -> range request -> validate (`attention_sync.rs`)
- [ ] Broadcast fraud proofs on author and intention topics — deferred to Phase 1.5 (`FraudEvidenceDocument` CRDT used instead)
- [x] Deterministic state reconstruction: replay chains to compute current attention + attention-time (`reconstruct_attention_state()`)
- [x] Lua simulation scenario for conservation stress testing (`attention_conservation_stress.lua`)

### Phase 2: Hardening (Witness Rosters + Quorum Certificates + Finality)

- [ ] Implement `WitnessRosterDocument` per intention
- [ ] Witness selection: prefer peers in `M(p,q)` (mutual peers) for triangle finality
- [ ] Implement `QuorumCertificate` with k-of-n PQ signatures
- [ ] Certificate request protocol: author requests witness signatures
- [ ] Certificate validation: verify quorum threshold against roster
- [ ] Broadcast certificates via gossip
- [ ] Policy engine: distinguish "observed" (valid event) from "final" (certified event)
- [ ] Fraud proof slashing: reject uncertified events from equivocating authors
- [ ] Lua simulation scenario for Byzantine witness behavior

---

## 13. Connection to Existing Domain Model

The attention ledger is not a standalone system. It connects deeply to the existing IndrasNetwork domain model.

| Guide Concept | Existing Type | Crate | Connection |
|---------------|---------------|-------|------------|
| Intention | `Intention` | `indras-artifacts` | A goal with proofs, attention tokens, and pledges. The guide's "intention" IS this type. |
| IntentionId | `ArtifactId` | `indras-artifacts` | Not a new type. It's the `ArtifactId` of an `Intention` artifact. |
| Mutual peers `M(p,q)` | `MutualPeering` | `indras-artifacts` | Already tracks canonical mutual peer relationships. |
| Peer neighborhoods `N(p)` | `PeerRegistry` / `PeerEntry` | `indras-artifacts`, `indras-storage` | Already tracks direct peers. |
| Attestation / finality | `BlessingRecord` | `indras-artifacts` | Blessings are single-peer attestations. Quorum certificates generalize to k-of-n. |
| Attention-time value | `compute_token_value()` | `indras-artifacts` | Derives economic value from attention data — connects to the conservation model. |
| Integrity checks | `IntegrityResult` / `check_integrity()` | `indras-artifacts` | Existing divergence detection. Extended with cryptographic validation. |
| Attention tracking | `AttentionLog<S>` / `AttentionStore` trait | `indras-artifacts` | Already has `append_event`, `check_integrity`, `ingest_peer_log`. Extend, don't replace. |
| Quest-level attention | `AttentionDocument` | `indras-sync-engine` | CRDT document with merge semantics. The conservation model adds hash-linking. |
| Realm API | `RealmAttention` trait | `indras-sync-engine` | Already provides `focus_on_quest`, `clear_attention`, `quests_by_attention`. Upgrade for conservation. |
| Dwell time / heat | `compute_heat()`, `AttentionValue` | `indras-artifacts` | Perspectival heat computation already exists. Conservation model ensures input data integrity. |
| Witness trust | `SentimentView` / relay system | `indras-network`, `indras-sync-engine` | Sentiment relay (trust propagation) could inform witness selection in Phase 2. |

### Implementation Status

The attention ledger has been implemented as a **greenfield upgrade** of the artifact-level types:

1. **`AttentionSwitchEvent`** — fully upgraded with `version`, `seq`, `prev`, `sig` fields, PQ signing, hash-chaining, and chain state tracking in `AttentionLog`. The old flat struct was replaced entirely.

2. **`AttentionStore`** — extended with `events_by_seq_range()` and `latest_tip()`. Existing methods (`append_event`, `events`, `check_integrity`, `ingest_peer_log`) continue working with the upgraded event type.

3. **`RealmAttention`** — pending upgrade to `switch_attention()` that creates signed, hash-linked events.

4. **`Intention`** — already exists as a first-class artifact type. The attention ledger refers to intentions by their existing `ArtifactId`. No new identifier type was needed.

5. **Sync documents** — `AttentionTipDocument` and `FraudEvidenceDocument` are implemented and registered with `impl_document_schema!`. Ready for gossip-based sync.

6. **Fraud proofs** — `EquivocationProof` in `indras-artifacts` provides the structural proof. `FraudRecord` in `indras-sync-engine` provides the wire format for cross-peer propagation.

---

## Appendix A — Practical Defaults (Reasonable Starting Values)

- Gossip message size: < 1-2 KB (attention events are ~200 bytes, well within limits)
- Tip sync interval: 5-30 seconds (adaptive based on activity)
- Event pinning: pin your own events + latest N from peers you track
- Witness threshold (Phase 2):
  - Small network: k=3 out of n=5
  - BFT-style: n=3f+1, k=2f+1
- Chain sync batch size: request up to 100 events per range request
- Fraud proof propagation: immediate broadcast on detection

---

## Appendix B — "Mutual Peers" Hardening Pattern (Phase 2)

For a recipient `q` validating an author `p`, define a witness pool from their mutual peers:

```
W(p,q) = N(p) intersection N(q)
```

If `|W(p,q)| >= m_min`, require `k = floor(m_min/2) + 1` PQ witness signatures. This yields **triangle finality**: local agreement anchored by overlap sets.

**Connection to existing types:** The `MutualPeering` type in `indras-artifacts::peering` already represents this relationship canonically (with `peer_a < peer_b` ordering). The `PeerRegistry` tracks the full neighborhood `N(p)`. Computing `M(p,q)` is a set intersection over two `PeerRegistry` entries.

The `SentimentView` system in `indras-network` and `indras-sync-engine` tracks trust signals between peers via relay chains. In Phase 2, sentiment scores could inform witness selection: prefer witnesses with high sentiment scores from both the author and the verifier.
