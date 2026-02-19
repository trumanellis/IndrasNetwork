# Plan: Sentiment Trust System + Substack Article

## Overview

Two parallel tracks executed simultaneously:
1. **Implementation**: Contact-scoped sentiment with second-degree relay and blocking cascade
2. **Article**: Approachable Substack piece for general tech audience using the immune system analogy

---

## Track 1: Implementation

### Phase 1 — Data Model Changes

**File: `crates/indras-network/src/contacts.rs`**

1. Add `ContactEntry` struct:
   ```rust
   #[derive(Serialize, Deserialize, Clone, Debug, Default)]
   pub struct ContactEntry {
       pub sentiment: i8,      // -1 = don't recommend, 0 = neutral, 1 = recommend
       pub relayable: bool,    // whether this sentiment can be relayed to second-degree contacts
   }
   ```

2. Change `ContactsDocument` from:
   ```rust
   pub contacts: BTreeSet<MemberId>,
   ```
   to:
   ```rust
   pub contacts: BTreeMap<MemberId, ContactEntry>,
   ```

3. Update all `ContactsDocument` methods:
   - `add()` → inserts with `ContactEntry { sentiment: 0, relayable: true }`
   - `remove()` → unchanged (BTreeMap::remove returns Option)
   - `contains()` → use `contacts.contains_key()`
   - `list()` → use `contacts.keys().copied().collect()`
   - `len()` / `is_empty()` → unchanged (BTreeMap has these)

4. Add new methods to `ContactsDocument`:
   - `set_sentiment(member_id, sentiment: i8)` — clamp to [-1, 1]
   - `get_sentiment(member_id) -> Option<i8>`
   - `set_relayable(member_id, relayable: bool)`
   - `contacts_with_sentiment() -> Vec<(MemberId, i8)>`

### Phase 2 — ContactsRealm API

**File: `crates/indras-network/src/contacts.rs`**

5. Add methods to `ContactsRealm`:
   - `update_sentiment(member_id, sentiment: i8) -> Result<()>` — update and sync
   - `get_sentiment(member_id) -> Option<i8>`
   - `get_contact_entry(member_id) -> Option<ContactEntry>`
   - `set_relayable(member_id, relayable: bool) -> Result<()>`

### Phase 3 — Blocking Cascade

**File: `crates/indras-network/src/network.rs`**

6. Add `block_contact(member_id) -> Result<Vec<RealmId>>` method to `IndrasNetwork`:
   - Remove contact via `contacts_realm.remove_contact()`
   - Find all peer-set realms containing the blocked member (scan `peer_realms` map)
   - Call `leave_realm()` for each affected realm
   - Return list of realm IDs that were left (for event emission)

7. Update `remove_contact` in `ContactsRealm` to NOT cascade (keep it as simple removal). Blocking is a separate, stronger operation that lives at the network level.

### Phase 4 — Second-Degree Sentiment Relay

**File: `crates/indras-network/src/sentiment.rs`** (new file)

8. Create `SentimentRelay` module:
   ```rust
   pub struct RelayedSentiment {
       pub about: MemberId,
       pub sentiment: i8,
       pub relay_source: MemberId,  // which of MY contacts relayed this
       pub degree: u8,              // 1 = direct, 2 = relayed
   }

   pub struct SentimentView {
       pub direct: Vec<(MemberId, i8)>,       // my contacts' ratings of target
       pub relayed: Vec<RelayedSentiment>,     // second-degree signals
   }
   ```

9. Add `query_sentiment(about: MemberId) -> SentimentView` to `IndrasNetwork`:
   - Collect direct sentiment from own contacts who know the target
   - For each contact with `relayable: true`, query THEIR contacts' sentiment about target
   - Apply attenuation (second-degree signals marked as degree 2)
   - Return aggregated view without exposing second-degree contact identities

10. Relay mechanism: The sentiment query works through the existing document sync. Each peer's ContactsDocument is synced within their contacts realm. To read a contact's contacts (for relay), we need read access to their contacts document. This requires:
    - A new `SentimentRelayDocument` per peer, published in the contacts realm
    - Contains: `BTreeMap<MemberId, i8>` of relayable sentiments only
    - Peers publish this document; contacts can read it
    - This avoids exposing the full contact list — only relayable sentiment ratings are shared

### Phase 5 — Event Types for Viewer

**File: `crates/indras-realm-viewer/src/events/types.rs`**

11. Add new event types:
    ```
    SentimentUpdated { tick, member, contact, sentiment }
    ContactBlocked { tick, member, contact, realms_left: Vec<String> }
    RelayedSentimentReceived { tick, member, about, sentiment, via }
    ```

**File: `crates/indras-realm-viewer/src/state/contacts_state.rs`**

12. Extend `ContactsState`:
    - Add `sentiments: HashMap<String, HashMap<String, i8>>` (member -> contact -> sentiment)
    - Add `relayed_sentiments: HashMap<String, Vec<RelayedSentiment>>` (member -> relayed signals about others)
    - Process new event types in `process_event()`

### Phase 6 — Lua Scenario for Testing

**File: `simulation/scripts/scenarios/sentiment_trust.lua`** (new file)

13. Write a simulation scenario that demonstrates:
    - Network of ~8 peers forming contacts and peer-set realms
    - Sentiment ratings accumulating over time
    - A "bad actor" peer receiving negative sentiment from multiple contacts
    - Blocking cascade removing bad actor from shared realms
    - Second-degree relay warning a peer about the bad actor before direct interaction
    - Visualization of topology evolution through the viewer

---

## Track 2: Substack Article

**File: `articles/your-network-has-an-immune-system.md`**

### Structure

1. **Opening hook** — "What if your social network could protect itself the way your body does?"
   - No technical jargon. Start with the everyday experience of a toxic person entering a friend group.

2. **The problem with moderation** — Brief section on why centralized moderation (content moderators, report buttons, trust & safety teams) doesn't scale and creates power asymmetries.

3. **How your body solves this** — Accessible explanation of the immune system:
   - Local detection (individual cells, not central brain)
   - Signal propagation (inflammation, cytokines)
   - Graduated response (innate → adaptive)
   - Memory (never forget a threat)

4. **A network that works the same way** — Map each immune concept to the sentiment system:
   - Each person rates their contacts: recommend, neutral, don't recommend
   - You only see ratings from people YOU trust (contact-scoped sentiment)
   - Friends-of-friends can relay warnings (second-degree relay)
   - Soft containment (negative sentiment prevents new group formation)
   - Hard isolation (blocking cascades through all shared spaces)
   - No moderator needed — the response is emergent

5. **What about coordinated attacks?** — Address the Sybil/brigading concern:
   - Fake accounts can't influence you because you don't know them
   - Your immune system only listens to YOUR cells
   - The autoimmune problem (groupthink) and why the gradient (-1 to +1) helps

6. **What emerges** — The topology is alive:
   - Communities form along trust gradients
   - Bad actors get naturally isolated
   - No one decided this centrally — it just happened

7. **Closing** — Tie back to Indra's Net philosophy (connects to the existing article). Every node reflects every other node, and now every node protects every other node.

### Tone guidelines
- Conversational, not academic
- Use concrete scenarios with futuristic names (Zephyr, Nova, Sage, etc.)
- No code snippets — this is for people who use apps, not build them
- Include 1-2 simple diagrams (describe in markdown, create SVG separately)
- ~1500-2000 words

---

## Execution Order

These two tracks run in parallel:

| Step | Implementation | Article |
|------|---------------|---------|
| 1 | Phase 1-2: Data model + API | Sections 1-3: Hook, problem, biology |
| 2 | Phase 3: Blocking cascade | Sections 4-5: Network mapping, attacks |
| 3 | Phase 4: Second-degree relay | Sections 6-7: Emergence, closing |
| 4 | Phase 5-6: Events + Lua scenario | Final edit pass, diagrams |

---

## Files Changed (Implementation)

| File | Change |
|------|--------|
| `crates/indras-network/src/contacts.rs` | ContactEntry struct, BTreeMap migration, new sentiment methods |
| `crates/indras-network/src/network.rs` | block_contact() with cascade, query_sentiment() |
| `crates/indras-network/src/sentiment.rs` | New file: RelayedSentiment, SentimentView, SentimentRelayDocument |
| `crates/indras-network/src/lib.rs` | Add `mod sentiment;` |
| `crates/indras-realm-viewer/src/events/types.rs` | New event types |
| `crates/indras-realm-viewer/src/state/contacts_state.rs` | Sentiment + relay tracking |
| `simulation/scripts/scenarios/sentiment_trust.lua` | New scenario |

## Files Created (Article)

| File | Description |
|------|-------------|
| `articles/your-network-has-an-immune-system.md` | Main article |
| `articles/images/sentiment-topology.svg` | Diagram: sentiment gradient shaping topology |
| `articles/images/blocking-cascade.svg` | Diagram: blocking cascade through peer-set realms |
