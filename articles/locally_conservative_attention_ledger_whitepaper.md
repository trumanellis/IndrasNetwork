# A Locally-Conservative Attention Ledger

## A Peer-to-Peer System for Verifiable Attention Accounting Without Global Consensus

### Abstract

We describe a peer-to-peer protocol for maintaining a conserved global
measure of attention without reliance on a globally ordered blockchain.
Instead of achieving consistency through total ordering of all events,
the system enforces local conservation invariants and non-equivocation
guarantees at the level of authors and intention-scoped quorums.
Attention is represented as a conserved unit mass that moves between
collective intentions via cryptographically signed switch-events. Global
conservation emerges from local antisymmetric state transitions and
convergent replication of an append-only event set. We show that under
quorum-intersection assumptions, the system achieves safety properties
analogous to Byzantine fault tolerant ledgers, while avoiding global
consensus overhead.

------------------------------------------------------------------------

## 1. Introduction

Existing distributed ledgers achieve agreement by imposing a single
total order over all transactions. This guarantees consistency but
imposes scalability and latency costs.

We propose a system with different goals:

1.  Maintain a globally conserved quantity (attention mass).
2.  Prevent equivocation and double-counting.
3.  Avoid global total order.
4.  Achieve safety through local quorum certification.

Instead of agreeing on order, nodes agree on validity.

The core idea is:

If all state transitions locally conserve a quantity, and if the event
set converges without equivocation, then global conservation holds
without global consensus.

------------------------------------------------------------------------

## 2. System Model

### 2.1 Network Model

Let the peer network be an undirected graph:

G = (V, E)

-   V: peers
-   E: authenticated communication links

Each peer is identified by a public key.

Assumptions:

-   Asynchronous message delivery.
-   Eventual message propagation among honest peers.
-   At most f Byzantine peers in any quorum set.

------------------------------------------------------------------------

### 2.2 Attention State

Let I be the set of collective intentions.

Each peer p ∈ V is in exactly one attention state at any time:

a_p(t) ∈ I ∪ {⊥}

Define the indicator:

χ\_{p,I}(t) = 1 if a_p(t) = I 0 otherwise

Total instantaneous attention on intention I:

A_I(t) = Σ χ\_{p,I}(t)

Total global attention:

Σ A_I(t) = \|V\|

Thus attention is a conserved discrete mass of \|V\| units.

------------------------------------------------------------------------

## 3. Attention Switch Events

Attention moves only through switch-events.

### Definition 1 (Switch Event)

A switch-event is:

e = (p, n, t, I_from, I_to, h_prev, σ_p)

Where:

-   p: author
-   n: monotonically increasing sequence number
-   I_from, I_to: intentions
-   h_prev: hash of prior event in author chain
-   σ_p: signature of author over event contents

Each peer maintains a hash-linked chain of events.

------------------------------------------------------------------------

### 3.1 Local Conservation Law

Each event induces:

Δ_e A_I = -1 if I = I_from +1 if I = I_to 0 otherwise

Thus:

Σ Δ_e A_I = 0

Lemma 1 (Local Conservation): Every switch-event preserves total
attention mass.

------------------------------------------------------------------------

## 4. Event Set Convergence

Let:

E = ⋃ L_p

Where L_p is the set of events authored by p.

Peers maintain append-only event sets and deduplicate by hash.

Assumption A1 (Event Convergence): Honest peers eventually converge to
the same set of valid events.

------------------------------------------------------------------------

## 5. Global Conservation Theorem

Theorem 1 (Global Conservation):

If:

1.  All state transitions are switch-events,
2.  Events satisfy Lemma 1,
3.  Peers converge on the same event set E,

Then:

Σ A_I(t) = \|V\| for all t.

Proof:

A_I(t) = A_I(0) + Σ Δ_e A_I

Summing over I:

Σ A_I(t) = Σ A_I(0) + Σ Σ Δ_e A_I

But Σ Δ_e A_I = 0 for each event.

Therefore:

Σ A_I(t) = Σ A_I(0) = \|V\|

QED.

------------------------------------------------------------------------

## 6. The Equivocation Problem

A malicious peer may attempt to produce:

e ≠ e' with identical (p, n)

This is equivocation.

Without protection, different regions may accept conflicting histories.

------------------------------------------------------------------------

## 7. Local Quorum Certification

### 7.1 Witness Sets

For each intention I, define a witness set:

W_I ⊆ V

With:

\|W_I\| = 3f + 1

At most f Byzantine.

Definition 2 (Certified Event):

An event affecting intention I is certified if it contains 2f + 1 valid
signatures from W_I over its hash.

------------------------------------------------------------------------

## 8. Safety Under Quorum Intersection

Theorem 2:

If:

1.  \|W_I\| = 3f + 1,
2.  Certification requires 2f + 1 signatures,
3.  Honest witnesses sign at most one event per (p, n),

Then two conflicting events cannot both be certified.

Proof:

Two quorums of size 2f + 1 in a set of 3f + 1 must intersect in at least
f + 1 nodes.

At least one honest node is in the intersection.

Honest nodes do not sign conflicting events.

Therefore both cannot be certified.

QED.

------------------------------------------------------------------------

## 9. Fraud Proofs

Define fraud proof:

π = (e, e')

Where author identical, sequence identical, hash differs.

Fraud proofs are objectively verifiable and can be propagated.

Peers reject all uncertified events from equivocated authors.

------------------------------------------------------------------------

## 10. Finality Without Global Order

Finality is defined locally:

An event is final when certified by quorum.

No global total order is required.

Finality is:

-   Scoped to intention,
-   Enforced by quorum intersection,
-   Independently verifiable.

------------------------------------------------------------------------

## 11. Conclusion

We have described a distributed protocol in which:

-   Attention is a conserved discrete mass.
-   State transitions obey a local antisymmetric law.
-   Global conservation follows algebraically.
-   Safety against equivocation is provided by local quorum
    certification.
-   No global consensus or total ordering is required.

This system replaces global agreement over order with local agreement
over validity.
