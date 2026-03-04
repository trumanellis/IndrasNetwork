# Building the Attention Ledger: From Theory to Working Code

## The Problem

Every distributed ledger built so far tracks the same thing: money. Bitcoin, Ethereum, and their descendants all solve the same puzzle: how do you get thousands of computers around the world to agree on who owns what? Their answer is *global consensus* — every computer processes every transaction, and they all vote on a single shared history. That works, but it's slow and expensive, because every machine must do all the work.

But money is not the only scarce resource worth tracking.

Attention is scarce. You can only focus on one thing at a time. Right now, you're reading this article — which means you're *not* reading anything else. Unlike money, attention cannot be counterfeited, stockpiled, or inflated. It moves — from one thing to another — and the total never changes. That conservation is not a design choice. It is a physical constraint of human cognition.

The attention ledger tracks this resource. Not with a blockchain. Not with global consensus. With a conservation law — a mathematical rule that says the total can never change, like how energy is conserved in physics.

## The Core Idea

Imagine a room of 100 people. Each person holds exactly one glowing ball — their attention. At any moment, each person is tossing their ball toward some project on a shared board. Some projects have lots of balls flying toward them (many people are focused there). Others have none.

The key rule: **the total number of balls in the room is always 100.** Nobody can create a ball out of thin air. Nobody can destroy one. You can only move yours from one project to another.

When you switch your focus, you create a signed record — like writing in a diary that everyone can read:

> "I, Nova, am moving my attention from Project Aurora to Project Cypress. This is my 5th entry. Here's my signature proving it's really me."

In code, that record looks like this:

```
(author, seq, from, to, prev_hash, signature)
```

- **author**: who switched (Nova)
- **seq**: which entry in their diary (5th)
- **from**: what they stopped focusing on (Project Aurora)
- **to**: what they started focusing on (Project Cypress)
- **prev_hash**: a fingerprint of their previous diary entry (to prove the diary hasn't been tampered with)
- **signature**: a cryptographic proof that the author really wrote this

Each switch is a `(-1, +1)` pair: one project loses a unit of attention, another gains one. The total across all active participants never changes. Conservation is enforced by algebra, not by getting everyone to vote.

This is the key difference from blockchain:

| | Blockchain | Attention Ledger |
|---|---|---|
| **What's tracked** | Token ownership | Cognitive focus |
| **Scarcity source** | Protocol rules | Physical constraint (one focus at a time) |
| **Integrity mechanism** | Global transaction ordering | Local conservation law |
| **Agreement scope** | Everyone agrees on everything | Local groups verify local events |
| **Scaling** | Globally constrained throughput | Shards naturally along intention lines |

Two unrelated projects never need to coordinate. If Nova switches her focus between Project Aurora and Project Cypress, that has nothing to do with Bodhi switching between Project Eden and Project Sage. The system scales because verification happens only where attention actually moves.

## Formal Guarantees

The system makes three mathematical promises. These aren't just good ideas — they're provable properties. If the code follows the rules, these guarantees hold no matter what.

### Theorem 1: Global Conservation

**In plain language:** The total amount of attention in the system always equals the number of *active* participants. If 100 people are in the network, total attention is 100. If someone joins, total attention becomes 101. If someone leaves, it drops to 99. Between membership changes, no matter how many switches happen or in what order, the total cannot change.

**Why it's true:** Every switch takes one unit away from some project and gives one unit to another. That's `(-1) + (+1) = 0` change to the total. If every switch event changes the total by zero, the total is constant between membership changes. Genesis events (`from: None`, `to: Some`) add exactly `+1` when a participant joins; farewell events (`from: Some`, `to: None`) subtract exactly `-1` when they leave. The total always equals the number of currently active participants.

**Formally:** Let `A_I(t)` be the total attention on intention `I` at time `t`, and let `V(t)` be the set of active participants (those with a genesis but no subsequent farewell). If all state transitions are switch, genesis, or farewell events, and peers converge on the same event set, then:

```
Σ A_I(t) = |V(t)|    for all t
```

The proof is a telescoping sum: each switch event's contribution sums to zero, each genesis adds one, and each farewell subtracts one — matching exactly the change in `|V(t)|`.

### Theorem 2: Safety Under Quorum Intersection

**In plain language:** A cheater cannot claim to be in two places at once and get away with it — as long as enough honest witnesses are watching.

**The attack:** Imagine Nova tells one group "I'm focused on Project Aurora" and tells another group "I'm focused on Project Cypress" — both at the same time, both as her 5th diary entry. This is called *equivocation* (literally: speaking with two voices). It's the attention-ledger version of double-spending in cryptocurrency.

**The defense:** For each project, a group of *witnesses* is assigned to verify events. Think of them as notaries. Before an event counts as official, a majority of the witnesses for that project must co-sign it. The mathematical insight: if you need a majority from the same group to approve *both* conflicting claims, those two majorities must overlap — at least one person is in both groups. And an honest witness will only sign one version. So both versions can never both get enough signatures.

**Formally:** The classic BFT formulation uses witness sets of size `3f + 1` with a `2f + 1` threshold, guaranteeing overlap of `f + 1` nodes (at least one honest). The current implementation uses a simpler model: a strict-majority threshold `k = floor(n/2) + 1` over an arbitrary witness set of size `n`. This guarantees that any two quorums overlap by at least one node. If that node is honest — which holds when honest witnesses outnumber Byzantine ones — then both conflicting events cannot be certified. The BFT formulation provides a strictly stronger guarantee (overlap of `f + 1`, not just 1), and upgrading the threshold is straightforward future work.

### Fraud Proofs

**In plain language:** If someone *does* try to cheat, the evidence is undeniable and spreads to everyone.

When equivocation is detected — two diary entries with the same author and same sequence number but different content — the pair itself is the proof. "Here are two entries both signed by Nova, both claiming to be entry #5, saying different things." Anyone can verify this. The proof spreads through the network, and the cheater's uncertified events are rejected everywhere.

### Finality Without Global Order

**In plain language:** An event becomes permanent when enough witnesses sign it — not when it gets added to some global list.

There is no "block" that everyone waits for. There is no mining. An event is *final* when certified by its intention's witness quorum, independently verifiable by any peer. Two events in unrelated projects can become final at the same time without knowing about each other.

## What We Built

The implementation lives in two existing Rust crates (a *crate* is Rust's term for a library or package):

- **`indras-artifacts`** — The data types and rules. Event structures, hash-chain validation, post-quantum signatures, fraud proofs, and quorum certificates. Pure logic with no network dependency — you could run these on an airplane.
- **`indras-sync-engine`** — The networking layer. Sync protocol, CRDT documents (explained below), finality classification, and the API that applications use. Builds on the existing `Document<T>` sync infrastructure.

No new crates were needed. The system is organized in layers, where each layer only depends on the ones below it:

```
Layer 4: Realm API + Lua bindings        (realm_attention.rs, simulation/)
Layer 3: CRDT documents + sync protocol  (attention_sync.rs, certificate.rs, witness_roster.rs)
Layer 2: Chain validation + fraud proofs  (validate.rs, fraud.rs, certificate.rs)
Layer 1: Event types + PQ signing         (attention/mod.rs)
```

## Layer 1: Hash-Chained Events with Post-Quantum Signatures

Every attention switch creates an `AttentionSwitchEvent`. This is the fundamental building block — every other part of the system exists to create, validate, sign, sync, or certify these events.

Here's the actual Rust type (`indras-artifacts/src/attention/mod.rs`, 422 lines):

```rust
pub struct AttentionSwitchEvent {
    pub version: u16,              // Protocol version (currently 1)
    pub author: PlayerId,          // Who switched attention
    pub seq: u64,                  // Monotonically increasing sequence number
    pub wall_time_ms: i64,         // Wall-clock timestamp
    pub from: Option<ArtifactId>,  // Intention losing attention (None for genesis)
    pub to: Option<ArtifactId>,    // Intention gaining attention (None for farewell)
    pub prev: [u8; 32],            // BLAKE3 hash of previous event (zeros for genesis)
    pub sig: Vec<u8>,              // PQ signature (Dilithium3 / ML-DSA-65)
}
```

Let's walk through what each field does:

- **`version`**: Which version of the protocol created this event (so future upgrades can understand old events).
- **`author`**: The person who switched attention. Every event has exactly one author.
- **`seq`**: A counter that goes up by one with each event. Nova's first event is 0, her second is 1, her third is 2, and so on. Gaps are not allowed — if we see event 5 but not event 4, we know something is missing.
- **`wall_time_ms`**: When it happened, in milliseconds. Used for computing how long someone focused on something.
- **`from`**: The intention *losing* attention. `None` when this is someone's very first event (joining the network).
- **`to`**: The intention *gaining* attention. `None` when someone is leaving the network.
- **`prev`**: A 32-byte fingerprint (hash) of the previous event in this author's chain. This is the "chain" in hash-chain — each event points back to the one before it, like links in a necklace. If anyone modifies a past event, the fingerprint won't match and the tampering is immediately obvious.
- **`sig`**: A digital signature proving the author really created this event. Uses Dilithium3 (also called ML-DSA-65), a signature scheme that is resistant to attacks from quantum computers. Regular digital signatures (like the ones Bitcoin uses) could be broken by a sufficiently powerful quantum computer. These cannot.

**Hash chaining** is how we make the diary tamper-proof. Imagine each diary entry ends with a fingerprint of the previous entry. If someone sneaks in and changes entry #3, the fingerprint stored in entry #4 won't match anymore — and neither will #5, #6, or any later entry. The whole chain after the tampering breaks. In code, BLAKE3 (a fast cryptographic hash function) produces these fingerprints, and `event_hash()` computes the hash over the full event including its signature.

**Genesis and farewell events** handle joining and leaving. A genesis event (`seq: 0`, `prev: [0; 32]`, `from: None`) means "I'm joining the network and focusing on this intention" — it adds one unit of attention to the system. A farewell event (`to: None`) means "I'm leaving" — it removes one unit. Both are hash-chained into the author's log. Note that these are *not* `(-1, +1)` pairs: genesis is `(0, +1)` and farewell is `(-1, 0)`. They change the total, but they also change the membership count by exactly the same amount, so `total attention = active participants` remains true.

**Conservation is structural.** Notice there is no "amount" field on the event. You can't transfer 2 units or 0.5 units. A switch event moves exactly one unit from `from` to `to`. A genesis event introduces exactly one unit (`from: None`). A farewell removes exactly one unit (`to: None`). Conservation isn't checked after the fact; it's baked into the shape of the data.

## Layer 2: Chain Validation and Fraud Detection

Each author's chain of events is like a diary with rules. Chain validation (`attention/validate.rs`, 158 lines) checks that those rules are followed. There are four rules, and each one corresponds to a precondition of Theorem 1 (conservation):

1. **Signature validity** — The PQ signature verifies against the author's public key. (Proves the author really wrote this entry, not an impersonator.)
2. **Sequence continuity** — `seq` equals the previous `seq + 1`. No gaps, no skipping, no going backwards. (Proves no entries were deleted or inserted.)
3. **Hash linking** — `prev` equals the hash of the previous event. (Proves the chain hasn't been tampered with.)
4. **Attention continuity** — `from` matches the previous event's `to`. If your last event said you moved your focus *to* Project Cypress, your next event must say you're moving *from* Project Cypress. (Proves attention didn't teleport — it can only leave where it currently is.)

`validate_chain(events, public_key)` walks the full chain from the first event (genesis) to the last and checks every rule at every step. If anything is wrong, it returns a specific error explaining exactly what failed:

```rust
pub enum ValidationError {
    InvalidSignature,
    SequenceGap { expected: u64, got: u64 },
    PrevHashMismatch { seq: u64 },
    AttentionContinuity { seq: u64, expected: Option<ArtifactId>, got: Option<ArtifactId> },
    InvalidGenesis,
    EmptyChain,
}
```

If validation succeeds, it returns an `AuthorState` — a summary of where the author's chain stands: their latest sequence number, latest hash, and where their attention currently is.

**Equivocation detection** (`attention/fraud.rs`, 68 lines) catches cheaters. An `EquivocationProof` is the evidence — two conflicting events with the same author and sequence number:

```rust
pub struct EquivocationProof {
    pub author: PlayerId,
    pub seq: u64,
    pub event_a: AttentionSwitchEvent,
    pub event_b: AttentionSwitchEvent,
}
```

Think of it like catching someone who wrote two different versions of page 5 in their diary and showed each version to different people. The proof is just the two pages side by side — anyone can see they both claim to be page 5 by the same person, but say different things. Validation happens in two steps: `is_valid()` checks the structural claim (same author, same seq, different hashes), and `verify_signatures(public_key)` confirms the author actually signed both — proving deliberate fraud rather than a transmission error. Both checks must pass for a fraud proof to be actionable.

## Layer 2.5: Witness Certificates and Quorum Finality

This layer implements Theorem 2 — the guarantee that cheaters can't get away with it.

**Witness rosters.** For each intention (project), a set of peers is designated as witnesses — like assigning a jury. The `WitnessRosterDocument` (`indras-sync-engine/src/witness_roster.rs`, 174 lines) maps each intention to its witness set. Witness selection uses `mutual_peers()` — peers that both the event author and the intention know about — and then computes the quorum threshold:

```
k = floor(|witnesses| / 2) + 1
```

This means "more than half." If there are 5 witnesses, you need at least 3 signatures. If there are 7, you need 4. Any two groups of "more than half" drawn from the same set must share at least one member — so if an honest witness is in the overlap, conflicting events can't both get enough signatures. (The classic BFT formulation uses a higher `2f+1` in `3f+1` threshold for stronger overlap guarantees; the current implementation uses strict majority as a simpler starting point.)

**Quorum certificates.** When an author creates an event, they ask witnesses to co-sign it. Each witness checks that the event is valid (proper signature, proper chain), then produces a `WitnessSignature` — their own PQ signature over the event's hash. When enough witnesses have signed, their signatures are collected into a `QuorumCertificate` (`indras-artifacts/src/attention/certificate.rs`, 418 lines):

```rust
pub struct QuorumCertificate {
    pub version: u16,
    pub event_hash: [u8; 32],
    pub intention_scope: ArtifactId,
    pub witnesses: Vec<WitnessSignature>,
}
```

A certificate is the event's "seal of approval." `validate_certificate()` checks five things: enough signatures are present (at least `k`), no witness signed twice, every signer is actually in the roster, every signer's public key is available, and every signature is cryptographically valid. Each check has a specific error:

```rust
pub enum CertificateError {
    InsufficientSignatures { have: usize, need: usize },
    SignerNotInRoster { signer: PlayerId },
    MissingPublicKey { witness: PlayerId },
    InvalidSignature { witness: PlayerId },
    DuplicateWitness { witness: PlayerId },
}
```

**Two-tier finality.** Events start as `Observed` (valid but not yet certified) and become `Final` once a quorum certificate is recorded:

```rust
pub enum EventFinality {
    Observed,  // Valid event, no quorum certificate yet
    Final,     // Valid event with k+ witness signatures
}
```

`classify_event_finality()` in `attention_sync.rs` checks whether a certificate exists with enough signatures. Finality is per-event, not per-block — because there are no blocks.

**Equivocation slashing.** If fraud evidence exists against an author, `is_slashed()` returns true and `filter_slashed_events()` rejects all their uncertified events. But certified events survive — the quorum certificate proves that a majority of honest witnesses verified the event *before* the cheating was discovered. The honest version is preserved; the fraudulent version is discarded.

## Layer 3: CRDT Sync Protocol

When multiple computers need to share data without a central server, they need a way to merge their information that always converges to the same result — regardless of what order messages arrive in, or if some messages arrive twice. A *CRDT* (Conflict-free Replicated Data Type) is a data structure designed for exactly this. The rule is simple: when two copies of the data meet, merge them using a function that gives the same result no matter which copy is "first." Eventually, all copies become identical.

Five CRDT documents handle the distributed state:

| Document | What It Stores | How Copies Merge |
|----------|---------------|------------------|
| `AttentionDocument` | Hash-chained events, grouped by author | Append any events the other copy has that we don't |
| `AttentionTipDocument` | Latest sequence number per author | Keep the higher sequence number |
| `CertificateDocument` | Quorum certificates, indexed by event hash | Combine witness signatures from both copies |
| `FraudEvidenceDocument` | Equivocation proofs | Keep all proofs from both copies |
| `WitnessRosterDocument` | Witness sets per intention | Combine witness lists from both copies |

All five use the existing `Document<T>` CRDT sync infrastructure. The merge functions are *commutative* (A merge B = B merge A), *associative* (order of three-way merges doesn't matter), and *idempotent* (merging the same data twice has no effect). These properties guarantee that all peers converge to the same state regardless of network timing.

**Chain sync protocol** (`attention_sync.rs`, 555 lines) adds an *anti-entropy* layer — a mechanism to actively find and fill gaps, rather than waiting passively. It works in five steps:

1. **Tip comparison** — Compare our latest sequence numbers against the `AttentionTipDocument` to see which authors have events we haven't seen yet.
2. **Gap detection** — `detect_gaps()` figures out exactly which events we're missing: "Lyra has events up to #7, but I only have up to #4 — I need #5, #6, and #7."
3. **Range request** — Ask peers for the specific missing events.
4. **Validation** — Check received events against the chain rules before accepting them.
5. **State reconstruction** — `reconstruct_attention_state()` rebuilds a summary of each author's chain from their validated events.

This protocol ensures that honest peers eventually converge to the same set of valid events — even if a peer joins late, even if messages arrive out of order, even if the network splits temporarily and reconnects.

## Layer 4: Realm API

The `RealmAttention` trait (`realm_attention.rs`, 346 lines) is the public interface that applications use. It hides all the complexity of chains, hashes, and certificates behind simple operations:

**Chain management:**
- `create_genesis_event(to, author, identity)` — Join the network with attention focused on an initial intention. Creates the first event, signs it with PQ cryptography, and advertises the new chain to peers.
- `switch_attention_conserved(from, to, author, identity, author_state)` — Switch focus. Creates a hash-chained, PQ-signed event. Automatically checks for equivocation and publishes fraud evidence if detected.

**Witness operations:**
- `request_witness_signature(event, scope, identity, witness_id, pubkey)` — As a witness, co-sign someone's event after verifying it's valid.
- `submit_certificate(cert, roster, k, public_keys)` — Submit a completed quorum certificate for distribution to all peers.

**Queries:**
- `get_member_focus(member)` — What is this person focused on right now?
- `get_quest_focusers(quest_id)` — Who is focused on this intention?
- `quests_by_attention()` — Rank all intentions by how much attention they're receiving.

The API surface is deliberately small. Chain validation, equivocation detection, and certificate verification all happen internally. Callers deal in intentions, members, and focus — not hashes and sequence numbers.

## Proof It Works: Test Results

### Unit and Integration Tests

67+ automated test functions across both crates verify that every rule is enforced:

- **Chain validation**: sequence gaps are caught, hash mismatches are caught, invalid signatures are caught, attention-teleportation is caught
- **Equivocation detection**: fraud proofs are correctly constructed, both forked signatures are verified
- **Certificate validation**: quorum thresholds are enforced, non-roster signers are rejected, duplicate witnesses are rejected
- **CRDT merges**: tip documents keep the max, certificates merge their witness lists, fraud evidence accumulates, rosters combine
- **Gap detection**: empty documents, one peer ahead of another, fully-synced peers
- **State reconstruction**: single author, multiple authors, graceful handling of invalid chains
- **Finality**: observed vs. final classification, edge cases near the quorum threshold
- **Slashing**: clean authors pass, fraudulent authors are blocked, certified events survive slashing

### Live-Node End-to-End Tests

Seven test scenarios run against real networked nodes — not simulations, but actual computers communicating over QUIC (a modern encrypted transport protocol). Each test creates an isolated multi-node network, exercises a specific feature, syncs state, and verifies correctness on *every* node.

| Test | Nodes | What It Proves |
|------|-------|----------------|
| `live_attention_basics` | 3 | Focus, clear, ranking, and CRDT sync of attention state |
| `live_attention_chains` | 3 | Genesis events, PQ-signed hash chains, multiple authors coexisting |
| `live_equivocation_slashing` | 4 | Fork detection, fraud evidence spreading to all nodes, certified vs. slashed events |
| `live_witness_certificates` | 5 | Witness co-signing, quorum certificate assembly, finality classification |
| `live_late_joiner_sync` | 3 | A node joining late catches up on the full history automatically |
| `live_farewell_events` | 3 | Chained departure events (`to: None`), synced to peers |
| `live_byzantine_witnesses` | 4 | Insufficient signatures rejected, invalid witness keys rejected |

**All seven pass end-to-end with real network transport.**

The `live_equivocation_slashing` test is the most comprehensive: it creates a legitimate chain, forks it with a conflicting event at the same sequence number, detects the equivocation, propagates fraud evidence to all four nodes, then demonstrates that a certified event survives slashing while the uncertified fork is rejected. This exercises Theorem 2's guarantee — the "cheaters can't win" proof — end-to-end across a real network.

The `live_late_joiner_sync` test validates the convergence guarantee: a node that joins after events have already been created receives the complete chain history through anti-entropy, with every hash-chain link verified on arrival. It proves the system works even when participants show up late.

## Scope and Numbers

| Component | File | Lines | Purpose |
|-----------|------|-------|---------|
| Event types + signing | `attention/mod.rs` | 422 | Core `AttentionSwitchEvent`, hash-chaining, PQ signing |
| Chain validation | `attention/validate.rs` | 158 | Sequence, hash, signature, continuity checks |
| Fraud proofs | `attention/fraud.rs` | 68 | `EquivocationProof` detection and verification |
| Quorum certificates | `attention/certificate.rs` | 418 | `QuorumCertificate`, k-of-n validation |
| Witness selection | `attention/witness.rs` | 141 | `mutual_peers()`, `select_witnesses()` |
| Chain sync protocol | `attention_sync.rs` | 555 | Gap detection, finality, slashing |
| Attention CRDT | `attention.rs` | 656 | `AttentionDocument` with chain storage |
| Tip document | `attention_tip.rs` | 91 | Anti-entropy tip advertisement |
| Realm API | `realm_attention.rs` | 346 | Public API for chain management + witnesses |
| Certificate CRDT | `certificate.rs` (sync-engine) | 227 | Certificate distribution and merge |
| Witness roster CRDT | `witness_roster.rs` | 174 | Per-scope roster tracking |
| Lua E2E tests | 7 `live_*.lua` files | 1,118 | End-to-end network tests |

**Totals:** ~3,250 lines of new Rust across 11 files, ~1,100 lines of Lua tests across 7 files. Seven commits on the branch. 24 of 26 implementation guide checklist items complete (92%).

## What's Deferred

- **Dedicated gossip broadcast.** Events and fraud proofs currently spread through the CRDT sync mechanism — when two peers connect, they exchange whatever the other is missing. This works correctly but is passive. A dedicated gossip layer that actively pushes new events to peers would be faster at scale.
- **Out-of-order event queuing.** Events currently arrive in order thanks to CRDT sync. A production system with unreliable connections may need a buffer to hold events that arrive out of sequence until the missing ones show up.
- **Anti-Sybil economics.** The conservation law holds regardless of identity — but one person running ten fake identities gets ten units of attention. Proof-of-humanness or economic defenses are needed to make the "one person, one unit" assumption meaningful. This is identity-layer work, outside the scope of the ledger itself.
- **BFT quorum threshold.** The current implementation uses strict majority (`floor(n/2) + 1`), which guarantees two quorums overlap by at least one node. The classic BFT formulation (`2f+1` in `3f+1`) guarantees overlap of `f+1` nodes — a strictly stronger guarantee. Upgrading the threshold is straightforward once `f` is known per witness set.
- **Production witness selection.** Witness rosters are currently assigned manually. Automatic selection based on network topology, reputation, or stake is future work.
- **Attention gratitude.** The full vision includes a cycle where completing work on a project earns redirected attention from others — attention as a non-monetary reward for contribution. The ledger tracks attention flow; the gratitude mechanism that closes the loop is not yet implemented.

## From Theory to Code: A Mapping

Every formal guarantee has a concrete implementation:

| Formal Model | Implementation |
|---|---|
| **Lemma 1** (Local Conservation) — each switch preserves total attention mass | The `from`/`to` structure of `AttentionSwitchEvent`. No "amount" field exists. Switch events are `(-1, +1)`. Genesis is `(0, +1)` and farewell is `(-1, 0)` — these change the total to match the membership change. |
| **Theorem 1** (Global Conservation) — total attention equals number of active participants | `validate_chain()` enforces sequence continuity, hash linking, and attention continuity. Genesis adds `+1`, farewell subtracts `-1`, switches are zero-sum. With valid chains and converged event sets (via CRDT sync), `Σ A_I(t) = |V(t)|` holds. |
| **Theorem 2** (Safety Under Quorum Intersection) — conflicting events cannot both be certified | `validate_certificate()` enforces `k = floor(n/2) + 1` (strict majority). Two quorums overlap by at least one node. With honest majority, conflicting events cannot both be certified. The `live_equivocation_slashing` test demonstrates this end-to-end. |
| **Fraud Proofs** — equivocation is detectable and provable | `EquivocationProof` captures conflicting events. `check_equivocation()` detects them. `FraudEvidenceDocument` propagates proofs via CRDT. `filter_slashed_events()` enforces consequences. |
| **Finality Without Global Order** — events become permanent without a total ordering | `classify_event_finality()` returns `Observed` or `Final`. Finality is per-event and per-intention. No global clock, no block production, no leader election. |
| **Event Convergence** — honest peers converge to the same event set | Five CRDT documents with commutative, associative, idempotent merge functions, plus anti-entropy gap detection. The `live_late_joiner_sync` test verifies this for late-joining nodes. |

## The Larger Picture

The attention ledger is not a financial system. It does not track tokens, balances, or ownership. It tracks where human cognitive energy is directed — and enforces a conservation law that makes that tracking meaningful.

Blockchain's insight was that global consensus enables trustless finance. The attention ledger's insight is that **you don't need global consensus if what you're tracking is locally conserved.** Conservation gives you the same integrity guarantee that consensus provides for money, but without the coordination cost.

Think of it this way: if you're tracking something that can be *copied* (like digital money), you need everyone to agree on a single history to prevent counterfeiting. But if you're tracking something that *physically can't be in two places at once* (like a person's focus), you just need local witnesses to confirm that it moved, and math to confirm that the total didn't change.

What this enables:

- **Verifiable collective focus.** Any peer can compute how much conserved attention flows toward any intention, without trusting a central authority.
- **Intention-scoped coordination.** Two unrelated projects never need to coordinate. The system shards naturally along intention lines.
- **Fraud accountability.** Equivocation is provable and punishable. You cannot claim to be focused on two things at once.
- **Quantum resistance.** All signatures use NIST-standardized post-quantum cryptography (ML-DSA-65). The system is hardened against quantum computer attacks from day one.
- **No mining, no staking, no gas.** Conservation is algebraic. There is nothing to mine and no fee to pay. The scarce resource is attention itself — and you already have exactly one unit of it.
