# NEXT: Where We Are and Where We're Going

## What We've Built

IndrasNetwork is a peer-to-peer system implementing the **Conservation of Attention** — a gift economy where attention is the scarce resource, tracked with the same rigor as a double-entry ledger.

### Foundation Layer (complete)

The P2P stack is fully operational across 26 crates:

- **Transport** (`indras-transport`): QUIC-based peer connections via iroh, with ALPN protocol negotiation and framed wire messages
- **Routing** (`indras-routing`): PRoPHET-based delay-tolerant routing for store-and-forward
- **Storage** (`indras-storage`): Tri-layer persistence (memory, disk, encrypted)
- **Sync** (`indras-sync`): Automerge CRDT replication — any two peers converge to the same state
- **Crypto** (`indras-crypto`): Post-quantum signatures (ML-DSA-65) and key encapsulation (ML-KEM-768), plus credential types for relay authentication
- **Network** (`indras-network`): High-level SDK wrapping all the above — HomeRealm, DM realms, peer discovery, gossip

### Attention Ledger (complete, with known weaknesses)

The core innovation — tracking who pays attention to what, and converting that into a verifiable gift economy:

- **Sync Engine** (`indras-sync-engine`): CRDT documents for intentions, attention events, blessings, tokens of gratitude. Attention is an append-only log of switch events with per-member focus windows.
- **Artifacts** (`indras-artifacts`): The formal layer — hash-chained, PQ-signed attention events with BFT quorum certificates, equivocation detection, and fraud proofs. This is the "hard proof" that attention was conserved.
- **Gift Cycle** (`indras-gift-cycle`): Dioxus UI implementing all 6 stages: Intention → Attention → Service → Blessing → Token → Renewal. The bridge orchestrates writes across home + DM realms with CRDT dedup.

### Recent Work (last ~2 weeks)

**Unified Attention Sync** (`c1c68dc`): Fixed the core routing bug where peers couldn't see each other's attention events. Events are now created once with a stable `event_id` and mirrored to all relevant realms (home + DM). CRDT dedup by `event_id` prevents double-counting. Source realm routing (`source_realm_id`) handles community vs home intentions correctly.

**Homepage System** (`worktree-homepage-feature`): Replaced the old `indras-profile` crate with `indras-homepage` — an axum HTTP server rendering a peer's public profile. Unified the visibility model: every displayable item is now an artifact with its own grant list. `AccessMode::Public` for world-visible, per-field grants for connections. Deleted the old dual-model (`Visible<T>`/`ViewLevel`) entirely.

**Relay Node** (`worktree-relay-node`): Evolved `indras-relay` from a blind store-and-forward server into an authenticated three-tier relay node. Storage tiers (Self, Connections, Public) with per-tier quotas, pin/TTL policies, and admin contact management. Credentials from `indras-crypto` authenticate peers. Tier-aware retrieval lets peers pull only what they're authorized to see.

---

## Where We're Going Next

### Phase 1: Quick Wins (single session)

These are small, independent fixes with immediate value:

**1a. Conservation warning in `insert_event`** — Add a `tracing::warn!` when a member switches focus without clearing first. Doesn't reject (CRDT merge must accept all events) but catches caller bugs immediately.
- File: `crates/indras-sync-engine/src/attention.rs`

**1b. Retry read-before-write** — The retry spawns at 2s/5s currently call `doc.update()` even when the event already arrived. Add a `doc.read()` check first to avoid redundant CRDT sync traffic.
- File: `crates/indras-gift-cycle/src/bridge.rs` (already partially done in the stash)

### Phase 2: Multi-Realm Card View (medium effort)

**Problem**: `build_intention_cards()` reads attention only from the home realm. Community intentions show 0 attention even when peers are actively focused.

**Fix**: Extract the "collect all attention events with dedup" pattern (already in `build_intention_view`) into a shared helper, and use it in `build_intention_cards` too. Requires adding `network: &IndrasNetwork` parameter and updating the one call site in `app.rs`.
- Files: `crates/indras-gift-cycle/src/data.rs`, `crates/indras-gift-cycle/src/app.rs`

### Phase 3: Lamport Clocks (medium effort, independent)

**Problem**: Events sort by wall clock (`timestamp_millis`). Peers with clock skew see sessions in wrong order.

**Fix**: Add `logical_clock: u64` to `AttentionSwitchEvent` with `#[serde(default)]` for backward compat. On create: `clock = max(seen) + 1`. On merge: advance past all remote clocks. Sort by `(logical_clock, timestamp_millis, event_id)`. Standard Lamport clock — gives causal ordering across all authors.
- File: `crates/indras-sync-engine/src/attention.rs`

### Phase 4: Formal Chain Integration (large effort, architecturally significant)

**The gap**: The gift cycle bridge creates lightweight `AttentionSwitchEvent` (UI layer) but never calls `create_genesis_event()` or `switch_attention_conserved()` from `RealmAttention` (formal layer). The whitepaper's conservation proof depends on the formal chain — hash-chained, PQ-signed events with quorum certificates.

**Plan**:
- Add `AuthorState` tracking and optional `PQIdentity` to `GiftCycleBridge`
- On every focus/clear, also create a formal chain event via `RealmAttention`
- Spawn background witness signature collection (best-effort, events start as Observed → become Final when certified)
- Graceful degradation when no PQ identity (skip formal chain, log warning)
- Files: `crates/indras-gift-cycle/src/bridge.rs`

### Phase 5: Garbage Collection (large effort, not urgent until scale)

**Problem**: Events accumulate forever in every DM realm. 100 peers = 101 copies per event.

**Plan**:
- Attention compaction: summarize events older than 30 days into `AttentionSummary` records (member, total_millis, per-intention breakdown)
- Only compact blessed events (attention already captured in tokens)
- Realm-scoped pruning: stop mirroring to inactive DM realms (peer offline 7+ days)
- Soft size limit: `should_compact()` at 10k events
- Files: `crates/indras-sync-engine/src/attention.rs`, `crates/indras-gift-cycle/src/bridge.rs`

### Beyond: Open Questions

- **Homepage ↔ Relay integration**: The homepage serves profiles over HTTP; the relay stores data in tiers. Should the relay serve homepage content directly? Or does the homepage pull from relay tiers?
- **Relay discovery**: How do peers find relays? Gossip? Hardcoded bootstrap? DNS?
- **Token economics**: Tokens of gratitude exist but have no exchange mechanism beyond pledge/withdraw. What does "renewal" look like in practice?
- **Multi-device**: The formal chain is per-author. How does a user with multiple devices maintain a single chain?

---

## Files to Modify (Phases 1-4)

| File | Phase |
|------|-------|
| `crates/indras-sync-engine/src/attention.rs` | 1a, 3, 5 |
| `crates/indras-gift-cycle/src/bridge.rs` | 1b, 4, 5 |
| `crates/indras-gift-cycle/src/data.rs` | 2 |
| `crates/indras-gift-cycle/src/app.rs` | 2 |

## Verification

After each phase: `cargo build -p indras-gift-cycle && cargo test -p indras-sync-engine --lib -- attention`
