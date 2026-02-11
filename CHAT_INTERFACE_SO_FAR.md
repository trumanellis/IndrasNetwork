# Chat Interface: Current Capabilities Report

*Snapshot as of 2026-02-07*

---

## Architecture Overview

```
UI Layer (Dioxus desktop app)
  App -> HomeRealmScreen / PeerRealmScreen
  |       |-- NoteEditorOverlay
  |       |-- QuestEditorOverlay
  |
State Layer (Signal<GenesisState>, 60+ fields, direct mutation)
  |
Sync Engine (extension traits on Realm)
  RealmMessages, RealmQuests, RealmNotes, RealmBlessings, ...
  |
Network Layer (IndrasNetwork)
  connect(), inbox_listener(), DM realms, identity codes
  |
Document Layer (Document<T>)
  Typed wrapper: read/update/refresh/listen
  postcard serialization, last-writer-wins replacement
  |
Node Layer (IndrasNode)
  iroh transport, Automerge sync, post-quantum crypto
  |
Storage Layer (CompositeStorage)
  Append-only event logs | redb KV | content-addressed blobs
```

---

## What Works Today

### Identity & Onboarding
- **Create identity** -- first-run genesis flow with display name entry
- **Share identity** -- copy `indra1...` bech32m code to clipboard
- **Add contacts** -- paste a peer's `indra1...` code to connect
- **Pass story** -- 23-slot identity protection ceremony

### Chat (PeerRealmScreen, right column)
- **Send text messages** to a contact via `realm.send_chat_text()`
- **Receive messages** via 3-second polling loop + CRDT document refresh
- **Message rendering** by type: Text, Image, System, Artifact, ProofSubmitted, BlessingGiven, ProofFolderSubmitted, Gallery, Reaction
- **Sender colors** -- 8 distinct member color classes in CSS
- **Message history** persists across sessions (stored in redb + Automerge events)

### Quests (home realm and shared realm)
- **Create quests** with title + markdown description
- **Full lifecycle**: Open -> Claimed -> Verified -> Completed
- **Markdown checklists** (`- [ ]` / `- [x]`) rendered inline
- **View/edit** in modal overlay with split markdown preview
- **Claims list** showing claimant, timestamp, proof indicator, verified badge

### Notes (home realm and shared realm)
- **Create notes** with title + content
- **View/edit** in modal overlay with live markdown preview
- **Raw text toggle** for viewing source

### Contacts (HomeRealmScreen sidebar)
- **Contact list** with display names and hex ID prefix
- **Sentiment indicators** (Recommend / Neutral / Blocked)
- **Click to navigate** to shared peer realm

### Other
- **Artifacts** listed from artifact storage
- **Tokens of gratitude** display section
- **Event log** raw event display
- **World view snapshots** saved every 30 seconds

---

## What's Partially Implemented (UI exists, logic incomplete)

| Feature | Status | Detail |
|---------|--------|--------|
| **Message editing** | UI only | Edit button + inline input exist, but edits are not persisted -- handler clears state without saving |
| **Action menu** ("+") | UI only | Three items listed (Artifact, Document, Proof of Service) -- all just close the menu with no handler |
| **Sentiment loading** | Hardcoded | UI shows indicators but values aren't loaded from the sentiment CRDT document (`// TODO`) |
| **Claimant names** | Hex only | Quest claims show raw hex IDs instead of resolved display names (`// TODO: resolve from contacts`) |

---

## What Exists in Network Layer But Has No UI

| Capability | Network method | Notes |
|------------|---------------|-------|
| **Encounter codes** | `create_encounter()` / `join_encounter()` | 6-digit codes for in-person peer discovery |
| **Introductions** | `introduce(peer_a, peer_b)` | Sends `__intro__` messages via DM realms |
| **Contact blocking** | `block_contact()` | Removes contact, cascades by leaving shared realms |

---

## P2P & Crypto Stack

| Layer | Technology | Status |
|-------|-----------|--------|
| Transport | iroh (Ed25519, relay-based P2P) | Working |
| Encryption | ML-KEM-768 (post-quantum key encapsulation) | Working |
| Signatures | ML-DSA-65 (post-quantum digital signatures) | Working |
| Sync | Automerge CRDT (5-second interval) | Working |
| DM realm derivation | blake3 hash of sorted member IDs | Working, deterministic |
| Inbox notifications | Background listener for `ConnectionNotify` | Working |

---

## Data Model (Messages)

```rust
type MessageId = [u8; 16];  // nanosecond timestamp + blake3(counter)

enum MessageContent {
    Text(String),
    System(String),
}

struct StoredMessage {
    id: MessageId,
    sender: MemberId,       // [u8; 32]
    content: MessageContent,
    timestamp_millis: u64,
}
```

Each realm has a named `"messages"` document of type `MessageDocument`. Sending appends a `StoredMessage`; receiving peers pick it up via document refresh.

---

## Important Architectural Note: Document Sync

`Document<T>` wraps typed state with read/update/refresh/listen semantics, but uses **last-writer-wins whole-document replacement** rather than structural CRDT merge. The entire `T` is serialized via postcard and replaced atomically on remote updates.

The underlying `NInterface` layer *does* use Automerge for event-level CRDT sync. But the `Document<T>` abstraction on top treats the typed payload as an opaque blob.

**Practical impact**: Two peers simultaneously adding messages could result in one message being lost. The 3-second polling interval and sequential nature of human chat make this unlikely but not impossible. For quests and notes, concurrent edits by different peers carry the same risk.

---

## Visual Design

**Theme**: "Minimal Terminal" -- dark, with serif + monospace typography.

| Token | Value | Usage |
|-------|-------|-------|
| Void black | `#0a0a0a` | Primary background |
| Gold | `#d4af37` | Accent, interactive elements |
| Cyan | `#00d4aa` | Secondary accent |
| Moss | `#5a7a5a` | Tertiary |
| Danger | `#ff3366` | Errors, destructive actions |

**Typography**: Cormorant Garamond (headings) + JetBrains Mono (code/IDs).

**Layouts**: Home realm is main + 280px sidebar. Peer realm is main + 360px chat panel.

---

## Key Files

| Area | Path | Lines |
|------|------|-------|
| Root component | `crates/indras-genesis/src/components/app.rs` | 691 |
| Home realm UI | `crates/indras-genesis/src/components/home_realm.rs` | 1076 |
| Peer realm UI | `crates/indras-genesis/src/components/peer_realm.rs` | 1198 |
| Note editor | `crates/indras-genesis/src/components/note_editor.rs` | 302 |
| Quest editor | `crates/indras-genesis/src/components/quest_editor.rs` | 346 |
| UI state | `crates/indras-genesis/src/state/mod.rs` | 451 |
| Message model | `crates/indras-sync-engine/src/message.rs` | 263 |
| Realm messages | `crates/indras-sync-engine/src/realm_messages.rs` | 97 |
| Document wrapper | `crates/indras-network/src/document.rs` | 675 |
| Direct connect | `crates/indras-network/src/direct_connect.rs` | 461 |
| Network API | `crates/indras-network/src/network.rs` | 1645 |
| Node layer | `crates/indras-node/src/lib.rs` | 1517 |
| Storage | `crates/indras-storage/src/composite.rs` | 456 |
| Styles | `crates/indras-genesis/assets/styles.css` | 2366 |
