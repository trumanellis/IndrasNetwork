# SyncEngine Build Plan

## Current State → Target State

### What Exists (solid foundations to keep)

| Layer | Crate | Status | Keep? |
|-------|-------|--------|-------|
| Core traits & types | `indras-core` | Built | **Keep** — generic identity, traits |
| Post-quantum crypto | `indras-crypto` | Built | **Keep** — ML-KEM, ML-DSA, ChaCha20 |
| Transport | `indras-transport` | Built | **Keep** — iroh/QUIC connections |
| Storage | `indras-storage` | Built | **Refactor** — adapt for artifact model |
| Routing | `indras-routing` | Built | **Keep** — store-and-forward still needed |
| DTN | `indras-dtn` | Built | **Keep** — delay-tolerant delivery |
| Sync | `indras-sync` | Built | **Refactor** — migrate Automerge docs → iroh docs |
| Gossip | `indras-gossip` | Built | **Refactor** — topic = artifact audience |
| Node | `indras-node` | Built | **Refactor** — coordinator for new model |
| Network SDK | `indras-network` | Built | **Major refactor** — Realm → Artifact model |
| App layer | `indras-sync-engine` | Built | **Rewrite** — Quest/Blessing/Token → Artifact/Exchange |
| UI | `indras-genesis` | Started | **Rewrite** — spatial browser |

### What the New Design Changes

The fundamental shift: **Realm → Artifact** as the primary organizing unit.

| Old Concept | New Concept | Relationship |
|-------------|-------------|--------------|
| Realm | Audience (on an artifact) | A realm was a container with members. An audience is a property of each artifact. |
| Document (Automerge) | Tree Artifact (iroh doc) | Same CRDT idea, but now every document is an artifact with steward + audience. |
| Artifact (static file) | Leaf Artifact (iroh blob) | Content-addressed blobs stay the same. |
| Quest | Request Artifact | A quest becomes an immutable request leaf with a mutable tree cloud around it. |
| QuestClaim | Offer tagged onto Request | An exchange offer linking claimant's proof artifact to the request. |
| Blessing | Stewardship Transfer | Completing a quest = transferring stewardship of a Token artifact. |
| Token of Gratitude | Artifact with attention history | A token is just an artifact whose value is computed from attention. |
| Attention (quest-scoped) | Attention Switch Events (artifact-scoped) | Broadened: attention tracks navigation through the entire artifact tree, not just quests. |
| Sentiment | Mutual Peering | Simplified: peers are mutual or not. Sentiment still relevant for subjective valuation. |
| Member | Player (iroh NodeID) | Same concept, different name. |

### What Stays Untouched

- **Post-quantum crypto** — the encryption model doesn't change.
- **Store-and-forward routing** — still needed for offline peers, packets are now artifact sync messages.
- **DTN strategies** — Epidemic and Spray-and-Wait still apply for artifact replication.
- **Deterministic ID derivation** — still used for DM realms, inbox, etc.
- **Sealed packets** — relay nodes still can't read content.

---

## The CRDT Decision: Automerge vs iroh Docs

The existing codebase uses **Automerge** for CRDT sync. The new spec proposes **iroh docs** (key-value CRDTs native to iroh). This is the most consequential architectural decision.

**Option A: Keep Automerge, use iroh only for transport.**
- Pro: No rewrite of sync layer. Automerge's rich data model (nested maps, lists, text) is more expressive than iroh's key-value docs.
- Con: Two sync protocols running in parallel (Automerge sync + iroh blob sync). More moving parts.

**Option B: Migrate to iroh docs for everything.**
- Pro: Single sync protocol. Audience = replica set maps directly. Simpler system.
- Con: iroh docs are key-value only (per-author, last-writer-wins per key). Less expressive than Automerge. Complex nested structures require manual key encoding.

**Option C (Recommended): Hybrid — iroh docs for structure, Automerge for rich documents.**
- Tree Artifact structure (references, metadata, ordering) → iroh docs. These are naturally key-value.
- Rich content that needs collaborative editing (shared notes, complex documents) → Automerge docs stored as blobs, synced via iroh.
- Attention logs → iroh docs (append-only per-player, perfect fit for key-value).
- This preserves the existing Automerge investment while gaining iroh's native audience scoping.

---

## Build Phases

### Phase 0: Foundation Alignment (1-2 weeks)

**Goal:** Update core types to reflect the Artifact model without breaking existing code.

**Tasks:**

0.1 — Add Artifact types to `indras-core`
```
indras-core/src/
  artifact.rs     # ArtifactId, ArtifactType, ArtifactMeta
  attention.rs    # AttentionSwitchEvent (broadened from quest-only)
  stewardship.rs  # Steward, Audience, StewardshipTransfer
```
- `ArtifactId` = enum of iroh `Hash` (leaf) or iroh `DocId` (tree)
- `ArtifactType` = Leaf(Image | Message | Request | File) | Tree(Conversation | Gallery | Document | Vault | Exchange)
- `ArtifactMeta` = { id, artifact_type, steward, audience, created_at }

0.2 — Add `PlayerIdentity` type wrapping iroh `NodeID`
- Keep existing `MemberId` as alias for now
- PlayerIdentity carries both Ed25519 (iroh) and PQ keys

0.3 — Define the Attention Switch Event as a first-class type
- Broaden from `(member, from_quest, to_quest)` to `(player, from_artifact, to_artifact, timestamp)`
- Existing quest-scoped attention events become a subset

**Deliverable:** Core types compile. Existing code still works alongside new types.

---

### Phase 1: Artifact Storage Layer (2-3 weeks)

**Goal:** Implement artifact CRUD on top of existing storage + iroh.

**Tasks:**

1.1 — Implement Leaf Artifact storage
- Leaf creation: hash content → store as iroh blob → return ArtifactId::Blob(hash)
- Leaf retrieval: fetch blob by hash (local first, then network)
- This largely wraps the existing blob storage in `indras-storage`

1.2 — Implement Tree Artifact storage
- Tree creation: create iroh doc → set metadata keys (steward, type, created_at) → return ArtifactId::Doc(doc_id)
- Child reference management: write `ref/{ordering_key}` entries pointing to child ArtifactIds
- This is new code but uses iroh's existing doc API

1.3 — Implement Audience management
- Audience = iroh doc replica set
- `add_to_audience(artifact_id, player_id)` → grant sync access
- `remove_from_audience(artifact_id, player_id)` → revoke sync access
- Steward-only authorization check

1.4 — Implement Stewardship
- `transfer_stewardship(artifact_id, new_steward)` → update metadata key
- Steward history as a log within the doc

1.5 — Artifact metadata document for Leaf artifacts
- Each blob gets a companion iroh doc holding steward, audience, type
- Link: doc contains `content_hash` key pointing to the blob

**Deliverable:** Can create, store, retrieve, and share artifacts. Can manage audience and stewardship.

**Test:** Create a leaf artifact (image), set audience to two players, verify both can fetch the blob. Transfer stewardship. Verify new steward can modify audience.

---

### Phase 2: Attention System (2 weeks)

**Goal:** Implement attention tracking and perspectival value computation.

**Tasks:**

2.1 — Player Attention Log
- Each player gets a personal iroh doc (attention log)
- Write: `{timestamp}` → `{ from: artifact_id, to: artifact_id }`
- Append-only, single-author (no conflicts)

2.2 — Attention Log Sharing (Mutual Peering)
- When two players mutually peer, they share attention log doc IDs
- Each player syncs the other's log (read-only)
- Peer registry: personal iroh doc mapping NodeID → attention log DocId

2.3 — Perspectival Heat Computation
- Given an artifact_id and a player's peer set:
  1. Collect all attention switch events referencing this artifact
  2. Filter to events from mutual peers who are in the artifact's audience
  3. Compute heat: `f(unique_peers, total_dwell_time, recency)`
- Expose as `compute_heat(artifact_id, peer_registry) -> f32`

2.4 — Mutual Witnessing / Integrity
- On sync, detect if a peer's attention log has diverged from previous replica
- Flag inconsistencies (events removed or modified)
- Expose divergence info to the UI layer

**Deliverable:** Players generate attention trails. Heat is computable per-artifact per-player. Log integrity is verifiable.

**Test:** Two players peer. Player A attends to artifact X. Player B computes heat on X and sees Player A's attention. Player A tries to rewrite log. Player B detects divergence.

---

### Phase 3: Exchange Protocol (1-2 weeks)

**Goal:** Implement artifact exchange as mutual stewardship transfer.

**Tasks:**

3.1 — Tagging (Exchange Offer)
- Player A creates a reference from their artifact to Player B's artifact
- This creates an Exchange Tree Artifact containing references to both + a conversation tree
- Both parties are in the exchange artifact's audience

3.2 — Acceptance / Rejection
- Accept: both stewardship fields update simultaneously (coordinated via the exchange doc)
- Reject: exchange artifact is marked closed
- No separate transaction primitive needed

3.3 — Migrate Quest workflow to Exchange
- Request Artifact = old Quest (immutable description + optional image)
- Offer = old QuestClaim (proof artifact tagged onto the request)
- Verify + Complete = accept the exchange (stewardship transfers)
- Blessing = the attention accumulated during the quest is captured as the token artifact's history

**Deliverable:** Full exchange workflow works. Quests expressible as Request + Offer + Exchange artifacts.

**Test:** Player A creates a Request. Player B tags an Offer onto it. Player A accepts. Stewardship of both artifacts transfers. Attention history is preserved.

---

### Phase 4: Spatial Browser UI (3-4 weeks)

**Goal:** Build the Dioxus spatial browser as defined in the UI architecture.

**Tasks:**

4.1 — Project scaffolding
```bash
dx new syncengine
```
- Set up Dioxus 0.7 with WebView renderer
- Configure Tailwind CSS
- Establish project structure per the Dioxus implementation guide

4.2 — iroh integration
- Boot iroh node at startup
- Spawn background service bridging iroh events → Dioxus signals
- Implement `use_iroh()` hook

4.3 — Core hooks
- `use_navigation()` — navigate + log attention switch (the critical hook)
- `use_artifact_view(id)` — resolve artifact into renderable view
- `use_heat(id)` — perspectival heat signal
- `use_peer_presence(id)` — who's here right now

4.4 — Spatial Shell
- Root component with breadcrumb trail, peer presence layer, steward controls
- Zoom in/out transitions (CSS animations)
- Context always visible

4.5 — Artifact Space components
- VaultSpace (personal root)
- ConversationSpace (branching message tree)
- GallerySpace (spatial image layout)
- RequestSpace (central request + orbiting offers)
- ExchangeSpace (negotiation between stewards)
- LeafView (full content display)

4.6 — Heat visualization
- CSS custom properties driven by Rust-computed heat values
- Glow, warmth, saturation as visual language
- Familiarity indicators for previously-visited artifacts

4.7 — Steward controls
- Audience management (add/remove players)
- Stewardship transfer gesture
- Exchange initiation (tag artifact onto another)

**Deliverable:** Working spatial browser. Can navigate the artifact tree, see heat, see peers, manage stewardship.

**Test:** Two players running the app. Player A creates artifacts, shares with Player B. Player B navigates the tree, sees heat from Player A's attention. Both see each other's presence. Exchange workflow completes through the UI.

---

### Phase 5: Integration & Migration (2 weeks)

**Goal:** Connect the new UI to the full network stack. Migrate remaining old concepts.

**Tasks:**

5.1 — Connect spatial browser to live iroh network
- Real peer discovery via gossip
- Real blob transfer over QUIC
- Real document sync across devices

5.2 — Migrate Token of Gratitude → Artifact model
- Token = a Leaf Artifact whose metadata includes the blessing/attention history
- Subjective valuation formula still applies, now using artifact attention + peer sentiment
- Steward chain = stewardship transfer history on the artifact

5.3 — Migrate Humanness → Artifact model
- Attestation = a Leaf Artifact signed by the attester
- Bioregional delegation tree = a Tree Artifact whose structure mirrors the delegation hierarchy
- Freshness computation unchanged

5.4 — Encounter Codes
- In-person discovery produces mutual peering (attention log exchange)
- Encounter = creation of mutual peer relationship + optional humanness attestation artifact

5.5 — DM / Inbox as Artifacts
- DM conversation = a Conversation Tree Artifact with deterministic ID derivation (same as existing `dm_realm_id`)
- Inbox = a personal Tree Artifact where connection requests appear as child references

**Deliverable:** Full system operational. Old Realm-based workflows expressible through Artifact model.

---

### Phase 6: Polish & Resilience (ongoing)

**Tasks:**

6.1 — Offline resilience testing
- Player goes offline, comes back, attention logs and artifacts sync correctly
- Store-and-forward routing delivers artifact sync messages through mutual peers

6.2 — Steward recovery
- Steward loses device
- Audience members hold replicas
- New device re-peers and recovers all stewarded artifacts

6.3 — Performance
- Heat computation caching (recompute on attention log changes, not per-render)
- Lazy blob loading profiling
- Tree artifact pagination for large conversations

6.4 — Mobile target
- Same Dioxus codebase, `dioxus/mobile` feature flag
- Touch gestures for spatial navigation (pinch zoom, swipe back)
- Compressed spatial layout for small screens

---

## Dependency Graph

```
Phase 0: Foundation Alignment
    │
    ├──► Phase 1: Artifact Storage
    │        │
    │        ├──► Phase 2: Attention System
    │        │        │
    │        │        ├──► Phase 3: Exchange Protocol
    │        │        │
    │        │        └──► Phase 4: Spatial Browser UI (can start with mock data)
    │        │                 │
    │        │                 ▼
    │        └────────────► Phase 5: Integration
    │                            │
    │                            ▼
    └──────────────────────► Phase 6: Polish
```

**Phases 2 and 4 can run in parallel.** The UI can start with mock/local data while the attention system is being built against iroh. They converge in Phase 5.

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| iroh docs API insufficient for tree structures | Medium | High | Fall back to Automerge for complex trees, use iroh docs only for metadata + attention logs |
| Blitz/Dioxus Native CSS animation gaps | High | Medium | Start with WebView renderer. Switch when Blitz matures. |
| iroh doc replica set management doesn't map cleanly to audience semantics | Medium | High | Build audience as a logical layer on top of iroh, managing sync tickets manually |
| Attention log volume at scale (hundreds of artifacts, many peers) | Medium | Medium | Prune old events, compute heat incrementally, cache aggressively |
| Two-way stewardship transfer atomicity | Medium | High | Use the exchange artifact as a coordination point. Both parties write "accept" to the exchange doc. Transfer executes when both writes are observed. |

---

## Crates to Deprecate (after migration)

Once the Artifact model is fully operational:

- `indras-sync-engine` Quest/Blessing/Token types → replaced by Artifact + Exchange
- `indras-sync` Automerge layer → partially replaced by iroh docs (keep for rich collaborative editing)
- `indras-network` Realm concept → replaced by Artifact audience model

These crates don't need to be deleted immediately. They can coexist during migration and be removed once all functionality is ported.

---

## Success Criteria

**Phase 1 complete when:** Two iroh nodes can create, share, and replicate artifacts with audience-scoped access control.

**Phase 2 complete when:** Attention switch events are logged, synced between mutual peers, and produce perspectival heat values.

**Phase 3 complete when:** Two players can complete a full exchange workflow (request → offer → accept → stewardship transfer).

**Phase 4 complete when:** A player can navigate the artifact tree through the spatial browser, seeing heat, presence, and breadcrumbs.

**Phase 5 complete when:** The full workflow (create artifact → share → attend → exchange) works end-to-end across devices over the real network.
