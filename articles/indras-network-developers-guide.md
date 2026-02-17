# The Indras Network Developer's Guide

A complete reference for building peer-to-peer applications with `indras-network`.

---

## Table of Contents

1. [Getting Started](#getting-started)
2. [Configuration & Presets](#configuration--presets)
3. [Identity](#identity)
4. [Realms](#realms)
5. [Direct Connect](#direct-connect)
6. [Encounters](#encounters)
7. [Messaging](#messaging)
8. [Documents](#documents)
9. [Members & Presence](#members--presence)
10. [Contacts](#contacts)
11. [Home Realm](#home-realm)
12. [Artifact Sharing](#artifact-sharing)
13. [Access Control](#access-control)
14. [Tree Composition](#tree-composition)
15. [Artifact Sync](#artifact-sync)
16. [Chat Messages](#chat-messages)
17. [Read Tracking](#read-tracking)
18. [Realm Aliases](#realm-aliases)
19. [Encryption](#encryption)
20. [Identity Export & Import](#identity-export--import)
21. [Blocking](#blocking)
22. [World View](#world-view)
23. [Error Handling](#error-handling)
24. [Escape Hatches](#escape-hatches)
25. [Re-exported Types](#re-exported-types)
26. [The Prelude](#the-prelude)

---

## Getting Started

### The Simplest Thing That Works

```rust
use indras_network::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let network = IndrasNetwork::new("~/.myapp").await?;
    let realm = network.create_realm("My Project").await?;
    println!("Invite: {}", realm.invite_code().unwrap());
    realm.send("Hello, world!").await?;
    Ok(())
}
```

`IndrasNetwork::new()` does everything: generates a cryptographic identity, opens local storage, starts the networking stack, connects to relay servers, and begins peer discovery. The path you pass becomes the data directory where keys, documents, and artifacts live on disk.

### Construction Methods

There are four ways to create a network instance:

**`IndrasNetwork::new(path)`** — The default. Uses `Preset::Default` configuration with the given data directory. Good for prototyping.

**`IndrasNetwork::preset(preset)`** — Returns a `NetworkBuilder` pre-configured for a specific use case (Chat, Collaboration, IoT, OfflineFirst). You must call `.data_dir()` and `.build()` on the builder.

**`IndrasNetwork::builder()`** — Returns a blank `NetworkBuilder` for full manual configuration.

**`IndrasNetwork::with_config(config)`** — Accepts a fully constructed `NetworkConfig` directly.

### Lifecycle

After construction, call `start()` to begin networking:

```rust
let network = IndrasNetwork::new("~/.myapp").await?;
network.start().await?;

// ... use the network ...

network.stop().await?;
```

`start()` initializes the transport layer, connects to relay servers, and begins listening for peers. `stop()` tears down all interfaces, cancels artifact syncs, and closes connections gracefully.

Check state with `is_running()`.

### First Run Detection

```rust
if network.is_first_run() {
    // Show onboarding UI
}
```

Returns `true` when the data directory was freshly created (no pre-existing identity). Useful for triggering setup wizards.

---

## Configuration & Presets

### Presets

Five presets configure the network for common use cases:

| Preset | Max Peers | Max Realms | PQ Crypto | Auto-Reconnect | Notes |
|--------|-----------|------------|-----------|-----------------|-------|
| `Default` | 64 | 32 | No | Yes | Balanced general use |
| `Chat` | 128 | 64 | No | Yes | Higher limits for messaging apps |
| `Collaboration` | 32 | 16 | No | Yes | Fewer, deeper connections |
| `IoT` | 8 | 4 | No | Yes | Minimal resource use |
| `OfflineFirst` | 64 | 32 | No | Yes | Aggressive caching, relaxed timeouts |

```rust
let network = IndrasNetwork::preset(Preset::Chat)
    .data_dir("~/.mychat")
    .display_name("Alice")
    .build()
    .await?;
```

### NetworkBuilder

The builder provides a fluent API for fine-grained control:

```rust
let network = IndrasNetwork::builder()
    .data_dir("~/.myapp")
    .display_name("Alice")
    .relay_servers(vec!["relay.example.com".into()])
    .enforce_pq_signatures(true)
    .passphrase("hunter2")
    .build()
    .await?;
```

Builder methods:

| Method | Type | Description |
|--------|------|-------------|
| `.data_dir(path)` | `&str` | **Required.** Where to store keys, docs, artifacts |
| `.display_name(name)` | `&str` | Human-readable name broadcast to peers |
| `.relay_servers(urls)` | `Vec<String>` | Custom relay server URLs |
| `.enforce_pq_signatures(bool)` | `bool` | Require ML-DSA-65 post-quantum signatures |
| `.passphrase(pass)` | `&str` | Encrypt the keystore with Argon2id + ChaCha20-Poly1305 |
| `.pass_story(story)` | `String` | Authenticate via a memorable story instead of a passphrase |

### NetworkConfig

The raw config struct that presets and builders produce:

```rust
pub struct NetworkConfig {
    pub preset: Preset,
    pub data_dir: PathBuf,
    pub display_name: Option<String>,
    pub relay_servers: Vec<String>,
    pub enforce_pq_signatures: bool,
    pub passphrase: Option<String>,
    pub pass_story: Option<String>,
}
```

### Authentication

Two mutually exclusive authentication modes protect the local keystore:

**Passphrase** — A traditional password. The keystore is encrypted with Argon2id key derivation feeding into ChaCha20-Poly1305.

**Pass Story** — A memorable narrative used as the key material. Same crypto underneath, but the input is a story rather than a password. See [Your Story Is Your Key](your-story-is-your-key.md).

If neither is set, the keystore is unencrypted on disk. For production apps, always set one.

---

## Identity

Every network instance has a cryptographic identity — a keypair generated at first run and stored in the data directory.

### MemberId

```rust
pub type MemberId = [u8; 32];
```

The 32-byte public key hash that uniquely identifies a peer across the network. This is the canonical identifier used everywhere — in member lists, access grants, message sender fields, and contact entries.

### Accessing Your Identity

```rust
let my_id: MemberId = network.id();
let member: Member = network.identity();
let name: &str = network.display_name();
```

`id()` returns your `MemberId`. `identity()` returns a full `Member` struct with display name and presence info. `display_name()` returns the human-readable name you set (or a default).

### Setting Display Name

```rust
network.set_display_name("Alice").await?;
```

Broadcasts the new name to all connected peers.

### IdentityCode

A human-shareable encoding of your `MemberId`:

```rust
let code: IdentityCode = network.identity_code();
println!("{}", code); // indra1qyz...k3m (~58 characters)
```

Identity codes use **bech32m** encoding with the `indra` human-readable prefix. They're designed to be copy-pasted, printed on business cards, or embedded in QR codes.

The URI format adds a scheme prefix and optional name:

```rust
let uri = code.to_uri();
// indra://indra1qyz...k3m
// indra://indra1qyz...k3m?name=Alice
```

Parsing back:

```rust
let code = IdentityCode::from_str("indra1qyz...k3m")?;
let code = IdentityCode::from_uri("indra://indra1qyz...k3m?name=Alice")?;
let member_id: MemberId = code.member_id();
let name: Option<&str> = code.display_name();
```

### Identity URI

A shorthand for the full URI string:

```rust
let uri: String = network.identity_uri();
// "indra://indra1qyz...k3m?name=Alice"
```

---

## Realms

A realm is a collaborative space where members communicate, share documents, and exchange artifacts. Under the hood, a realm maps to a gossip topic (an `iroh` interface) where all members publish and subscribe to messages.

### Creating a Realm

```rust
let realm = network.create_realm("My Project").await?;
```

This creates a new realm, generates an invite code, and returns a `Realm` handle. The creator is automatically the first member.

### Joining a Realm

```rust
let realm = network.join("indra:realm:abc123...").await?;
```

Parses an invite code string and joins the corresponding realm. Returns a `Realm` handle.

### Invite Codes

```rust
let invite: Option<&InviteCode> = realm.invite_code();
println!("{}", invite.unwrap());
// indra:realm:kFd8mQ...
```

Invite codes use the URI format `indra:realm:<base64-encoded-data>`. The encoded data contains the realm's gossip topic ID and, for artifact-backed realms, the `ArtifactId`.

Parsing invite codes:

```rust
let invite = InviteCode::from_str("indra:realm:abc123...")?;
let topic_id = invite.topic_id();
let artifact_id: Option<&ArtifactId> = invite.artifact_id();
```

### Listing Realms

```rust
let realms: Vec<Realm> = network.realms().await;
```

Returns all realms the local node has joined.

### Getting a Realm by ID

```rust
let realm_id: RealmId = /* ... */;
let realm: Option<Realm> = network.get_realm_by_id(realm_id).await;
```

`RealmId` is a type alias for the interface identifier (a `[u8; 32]`).

### Peer-Based Realms

Two shortcuts create or retrieve a realm for a specific peer:

```rust
// Get-or-create a DM realm with a peer
let realm: Realm = network.realm(peer_id).await?;

// Get an existing peer realm (returns None if not yet created)
let realm: Option<Realm> = network.get_realm(peer_id).await;
```

The realm ID for a peer-based realm is deterministically derived from both members' IDs using BLAKE3, so both sides always agree on the same realm.

### Leaving a Realm

```rust
network.leave_realm(realm_id).await?;
```

Removes the realm from local state and stops participating in its gossip topic.

### Realm Properties

```rust
let id: RealmId = realm.id();
let name: &str = realm.name();
let artifact_id: Option<&ArtifactId> = realm.artifact_id();
let invite: Option<&InviteCode> = realm.invite_code();
```

The `artifact_id` links the realm to its corresponding Tree artifact in the domain model. This was added in the unification — a realm IS a Tree artifact.

---

## Direct Connect

The direct connect system implements the "Identity IS Connection" pattern. Knowing someone's `MemberId` is sufficient to establish a connection — no server, no IP address, no port number.

### How It Works

Every peer has a deterministic **inbox realm** derived from their `MemberId` via BLAKE3:

```rust
// Internally:
fn inbox_realm_id(member_id: &MemberId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"indras:inbox:");
    hasher.update(member_id);
    *hasher.finalize().as_bytes()
}
```

When you want to connect to someone, you send a `ConnectionNotify` message to their inbox. They receive it and can accept the connection.

### Connecting

```rust
// Connect by MemberId
let realm = network.connect(their_member_id).await?;

// Connect by IdentityCode
let realm = network.connect_by_code("indra1qyz...k3m").await?;
```

Both methods:
1. Derive the peer's inbox realm ID
2. Send a connection notification
3. Wait for the peer to acknowledge
4. Create a shared DM realm
5. Return the `Realm` handle

### Key Exchange

Post-quantum key exchange happens automatically during connection. The system uses **ML-KEM-768** for key encapsulation:

```rust
pub struct PendingKeyExchange {
    pub peer_id: MemberId,
    pub our_encapsulation_key: Vec<u8>,
    pub status: KeyExchangeStatus,
    pub created_at: u64,
}

pub enum KeyExchangeStatus {
    Initiated,
    ResponseReceived { shared_secret: Vec<u8> },
    Completed,
    Failed(String),
}
```

The `KeyExchangeRegistry` (a CRDT document) tracks all pending and completed exchanges. You typically don't interact with this directly — it's managed automatically by `connect()` and `connect_by_code()`.

### Initiator Determination

When two peers connect, one must be the "initiator" (to avoid duplicate realms). This is determined by comparing `MemberId` bytes:

```rust
pub fn is_initiator(our_id: &MemberId, their_id: &MemberId) -> bool {
    our_id > their_id
}
```

Deterministic and symmetric — both sides always agree on who initiates.

---

## Encounters

Encounters enable in-person peer discovery using short spoken codes. Two people in the same room say "my code is 847293" and they're connected.

### Creating an Encounter

```rust
let handle: EncounterHandle = network.create_encounter().await?;
println!("Tell them: {}", handle.code()); // "847293"
```

The `EncounterHandle` contains a 6-digit code and the underlying gossip topic.

### Joining an Encounter

```rust
let code = "847293";
let handle: EncounterHandle = network.join_encounter(code).await?;
```

Both parties are now on the same gossip topic and can exchange identity information.

### Time Windows

Encounter codes are valid for 60-second windows. The current window is derived from the system clock:

```rust
fn current_time_window() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() / 60
}
```

The gossip topic is derived from the code AND the time window via BLAKE3, so the same 6-digit code maps to different topics at different times. This prevents code reuse attacks.

### Exchange Payload

When peers discover each other on the encounter topic, they exchange:

```rust
pub struct EncounterExchangePayload {
    pub member_id: MemberId,
    pub display_name: Option<String>,
    pub signing_key: Vec<u8>,
    pub kem_key: Vec<u8>,
}
```

This includes the ML-DSA-65 signing key and ML-KEM-768 encapsulation key for post-quantum secure communication.

### Introduction

After an encounter, formalize the connection:

```rust
network.introduce(their_member_id).await?;
```

This creates the DM realm, exchanges keys, and adds the peer as a contact.

---

## Messaging

### Sending Messages

```rust
// Simple text
realm.send("Hello!").await?;

// With a content builder
realm.send(Content::text("Hello!")).await?;

// Images
realm.send(Content::image(bytes, "photo.jpg", "image/jpeg")).await?;

// Artifacts
realm.send(Content::artifact(artifact_id, "report.pdf")).await?;

// Reactions
realm.react(message_id, "thumbsup").await?;

// Replies
realm.reply(parent_message_id, "I agree!").await?;
```

### The Content Enum

`Content` has 15 variants covering every type of message payload:

| Variant | Description | Construction |
|---------|-------------|--------------|
| `Text(String)` | Plain text | `Content::text("hello")` |
| `Binary { data, mime_type }` | Raw bytes with MIME | `Content::binary(data, "application/pdf")` |
| `Artifact { id, name }` | Reference to a shared artifact | `Content::artifact(id, "file.pdf")` |
| `Reaction { target, emoji }` | Emoji reaction to a message | `Content::reaction(msg_id, "heart")` |
| `System(String)` | System notification | `Content::system("Alice joined")` |
| `Extension { type_name, data }` | Custom typed payload | `Content::extension("myapp.poll", data)` |
| `Image { data, filename, mime_type }` | Inline image | `Content::image(bytes, "photo.jpg", "image/jpeg")` |
| `InlineArtifact { data, filename, mime_type }` | Embedded file | `Content::inline_artifact(bytes, "doc.pdf", "application/pdf")` |
| `Gallery { items, caption }` | Multiple images | `Content::gallery(items, Some("Vacation photos"))` |
| `ArtifactGranted { artifact_id, artifact_name, access_mode, granter }` | Access grant notification | Constructed internally |
| `ArtifactRecalled { artifact_id, artifact_name, recaller }` | Access revoked notification | Constructed internally |
| `RecoveryRequest(ArtifactRecoveryRequest)` | Request to recover artifacts | Constructed internally |
| `RecoveryManifest(RecoveryManifest)` | Recovery manifest response | Constructed internally |

**Extension** is the escape hatch for application-specific messages. The `type_name` is a namespaced string (e.g., `"myapp.poll"`) and `data` is arbitrary bytes.

### Gallery Items

```rust
pub struct GalleryItem {
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub thumbnail_data: Option<String>,
    pub artifact_hash: String,
    pub dimensions: Option<(u32, u32)>,
}
```

### Receiving Messages

```rust
use futures::StreamExt;

// Live stream of new messages
let mut messages = realm.messages();
while let Some(msg) = messages.next().await {
    println!("{}: {}", msg.sender_name(), msg.content.as_text().unwrap_or(""));
}
```

The `messages()` method returns an async `Stream` of `Message` structs.

### Message Struct

```rust
pub struct Message {
    pub id: MessageId,
    pub sender: MemberId,
    pub sender_name: String,
    pub content: Content,
    pub timestamp: u64,
    pub sequence: u64,
    pub reply_to: Option<MessageId>,
    pub references: Vec<ContentReference>,
}
```

Key fields:
- `id` — A unique `MessageId` (a `[u8; 32]` hash)
- `sequence` — Monotonically increasing per-realm counter, used for read tracking
- `reply_to` — Links to parent message for threading
- `references` — Additional content references (artifacts, other messages)

### Content Reference

```rust
pub struct ContentReference {
    pub ref_type: String,
    pub ref_id: Vec<u8>,
    pub display_name: Option<String>,
}
```

### Querying Messages

```rust
// All messages (loads from storage)
let all: Vec<Message> = realm.all_messages().await?;

// Messages after a sequence number
let recent: Vec<Message> = realm.messages_since(sequence_number).await?;

// Full-text search
let results: Vec<Message> = realm.search_messages("budget report").await?;
```

---

## Documents

Documents are CRDT-backed typed data structures that automatically synchronize across all realm members. They're the building block for shared application state.

### DocumentSchema Trait

Any type that implements `Serialize + DeserializeOwned + Default + Clone + Send + Sync` automatically gets a `DocumentSchema` implementation via a blanket impl:

```rust
impl<T> DocumentSchema for T
where
    T: Serialize + DeserializeOwned + Default + Clone + Send + Sync + 'static,
{
    fn name() -> &'static str {
        std::any::type_name::<T>()
    }
    fn default_value() -> Self {
        Self::default()
    }
}
```

This means you don't need to implement anything special — just derive the right traits:

```rust
#[derive(Default, Clone, Serialize, Deserialize)]
struct TodoList {
    items: Vec<TodoItem>,
    title: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct TodoItem {
    text: String,
    done: bool,
}
```

### Getting a Document

```rust
let doc: Document<TodoList> = realm.document("todos").await?;
```

The document is identified by name within a realm. If it doesn't exist yet, it's created with the schema's `default_value()`.

### Reading

```rust
// Async read (acquires lock)
let data: TodoList = doc.read().await;

// Blocking read (for sync contexts)
let data: TodoList = doc.read_blocking();

// Force refresh from storage
doc.refresh().await?;
```

### Writing

```rust
// Simple update
doc.update(|todos| {
    todos.items.push(TodoItem {
        text: "Buy groceries".into(),
        done: false,
    });
}).await?;

// Transaction (multiple operations)
doc.transaction(|todos| {
    todos.items.retain(|item| !item.done);
    todos.title = "Active Items".into();
}).await?;
```

Both `update` and `transaction` serialize the new state and broadcast it to peers. The difference is semantic — use `transaction` when you want to signal that multiple changes are atomic.

### Reactive Changes

```rust
use futures::StreamExt;

let mut changes = doc.changes();
while let Some(change) = changes.next().await {
    match change {
        DocumentChange::Updated(new_value) => {
            println!("Document updated: {:?}", new_value);
        }
        DocumentChange::Conflict(local, remote) => {
            // Handle merge conflict (rare with CRDTs)
        }
    }
}
```

The `changes()` stream fires whenever the document is updated — either locally or from a remote peer. A background listener watches for `DocumentEnvelope` messages on the realm's gossip topic and deserializes them into the typed value.

### Document Discovery

```rust
let names: Vec<String> = realm.document_names().await?;
let exists: bool = realm.has_document("todos").await?;
```

---

## Members & Presence

### Member Struct

```rust
pub struct Member {
    pub id: MemberId,
    pub display_name: String,
    pub is_online: bool,
    pub last_seen: Option<u64>,
}
```

### Member Events

```rust
use futures::StreamExt;

let mut events = realm.member_events();
while let Some(event) = events.next().await {
    match event {
        MemberEvent::Joined(member) => println!("{} joined", member.display_name),
        MemberEvent::Left(member_id) => println!("Someone left"),
        MemberEvent::Updated(member) => println!("{} updated", member.display_name),
        MemberEvent::Discovered(member) => println!("Found {}", member.display_name),
    }
}
```

`Discovered` fires when a peer is found on the gossip network but hasn't formally "joined" yet (e.g., they're still syncing).

### Listing Members

```rust
// Basic list
let members: Vec<Member> = realm.member_list().await?;

// With cryptographic details
let detailed: Vec<MemberInfo> = realm.member_list_with_info().await?;

// Count
let count: usize = realm.member_count().await?;
```

### MemberInfo

```rust
pub struct MemberInfo {
    pub id: MemberId,
    pub display_name: String,
    pub signing_key: Option<Vec<u8>>,     // ML-DSA-65
    pub kem_key: Option<Vec<u8>>,          // ML-KEM-768
    pub is_online: bool,
    pub last_seen: Option<u64>,
}
```

The `signing_key` is the post-quantum ML-DSA-65 public key. The `kem_key` is the ML-KEM-768 encapsulation key used for key exchange.

### Presence

```rust
let online: Vec<Member> = realm.online_members().await?;
let is_online: bool = realm.is_member_online(member_id).await?;
```

Presence is tracked via periodic heartbeats on the gossip topic. A member is considered offline if no heartbeat is received within the timeout window.

---

## Contacts

The contacts system provides a way to manage relationships with other peers across realms.

### Joining the Contacts Realm

```rust
let contacts_realm: ContactsRealm = network.join_contacts_realm().await?;

// Or retrieve if already joined
let contacts_realm: Option<ContactsRealm> = network.contacts_realm().await;
```

The contacts realm is a special realm where your contact list is stored as a CRDT document.

### Contact Entry

```rust
pub struct ContactEntry {
    pub member_id: MemberId,
    pub display_name: String,
    pub added_at: u64,
    pub status: ContactStatus,
    pub sentiment: i8,       // -1, 0, or 1
    pub relayable: bool,     // Whether this contact can relay messages
    pub notes: Option<String>,
}
```

### Contact Status

```rust
pub enum ContactStatus {
    Pending,    // Invitation sent, not yet accepted
    Confirmed,  // Both sides acknowledge the contact
}
```

### Sentiment

Sentiment is a simple -1/0/1 value:

| Value | Meaning |
|-------|---------|
| `-1` | Negative (blocked, muted) |
| `0` | Neutral (default) |
| `1` | Positive (trusted, favorited) |

```rust
contacts_realm.update_sentiment(&member_id, 1).await?; // Mark as trusted
contacts_realm.update_sentiment(&member_id, -1).await?; // Mark as blocked

// Query sentiment
let sentiment: Option<i8> = contacts_realm.get_sentiment(&member_id);

// Get contacts with sentiment values
let with_sentiment: Vec<(MemberId, i8)> = contacts_realm.contacts_with_sentiment();

// Get only relayable sentiments (for second-degree relay)
let relayable: Vec<(MemberId, i8)> = contacts_realm.relayable_sentiments();
```

### Managing Contacts

```rust
// Add a contact (default sentiment, no display name)
contacts_realm.add_contact(member_id).await?;

// Add a contact with a display name
contacts_realm.add_contact_with_name(member_id, Some("Alice".into())).await?;

// Remove a contact
contacts_realm.remove_contact(&member_id).await?;

// Check if a member is a contact
let is_contact: bool = contacts_realm.is_contact(&member_id).await;

// List all contact IDs (sync and async variants)
let contact_ids: Vec<MemberId> = contacts_realm.contacts_list();
let contact_ids: Vec<MemberId> = contacts_realm.contacts_list_async().await;

// Get the full entry for a contact
let entry: Option<ContactEntry> = contacts_realm.get_contact_entry(&member_id);

// Contact count
let count: usize = contacts_realm.contact_count();

// Toggle relayable
contacts_realm.set_relayable(&member_id, true).await?;

// Confirm a contact (Pending → Confirmed)
contacts_realm.confirm_contact(&member_id).await?;

// Check status
let status: Option<ContactStatus> = contacts_realm.get_status(&member_id);
let status: Option<ContactStatus> = contacts_realm.get_status_async(&member_id).await;
```

### ContactsDocument

Under the hood, contacts are stored in a `ContactsDocument` — a CRDT document synced within the contacts realm. You typically interact through `ContactsRealm` methods, but the raw document is accessible if needed.

---

## Home Realm

The home realm is your personal storage space. It's a deterministic realm derived from your `MemberId` that only you can write to. Other peers can read artifacts you've shared from it.

### Getting the Home Realm

```rust
let home: HomeRealm = network.home_realm().await?;

// Or get it if it exists
let home: Option<HomeRealm> = network.get_home_realm().await;
```

### Home Realm ID Derivation

The home realm ID is deterministic — derived from your `MemberId` via BLAKE3:

```rust
pub fn home_realm_id(member_id: &MemberId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"indras:home:");
    hasher.update(member_id);
    *hasher.finalize().as_bytes()
}
```

This means anyone who knows your `MemberId` can derive your home realm ID and request artifacts from it.

### Sharing Artifacts

```rust
// Share raw bytes
let artifact_id = home.share_artifact(
    "report.pdf",
    bytes,
    "application/pdf",
).await?;

// Share a file from disk
let artifact_id = home.share_file("/path/to/report.pdf").await?;

// Upload with explicit name and mime type
let artifact_id = home.upload(
    "photo.jpg",
    image_bytes,
    "image/jpeg",
).await?;
```

All three methods store the artifact, register it in the artifact index, and make it available for P2P sync.

### Retrieving Artifacts

```rust
let entry: Option<HomeArtifactEntry> = home.get_artifact(artifact_id).await?;
```

### HomeArtifactMetadata

```rust
pub struct HomeArtifactMetadata {
    pub id: ArtifactId,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub created_at: u64,
}
```

### Documents in Home Realm

```rust
let doc: Document<MySchema> = home.document("settings").await?;
```

The home realm supports typed documents just like regular realms.

### DM and Realm Artifact Registration

```rust
// Ensure a DM story artifact exists for a peer
let story_id = home.ensure_dm_story(peer_member_id).await?;

// Ensure a realm artifact exists
let artifact_id = home.ensure_realm_artifact(realm_id, "Project Alpha").await?;
```

These methods create Tree artifacts that represent conversations and realms in your home artifact tree.

---

## Artifact Sharing

### ArtifactId

```rust
pub enum ArtifactId {
    Blob([u8; 32]),  // Content-addressed immutable blob
    Doc([u8; 32]),   // CRDT document identifier
}
```

`Blob` variants are hashes of the content. `Doc` variants are deterministically derived identifiers for CRDT documents.

### Deterministic ID Generation

Several helpers generate deterministic IDs:

```rust
// Generate a random tree ID
let tree_id = generate_tree_id();

// Generate a content-addressed leaf ID from payload bytes
let leaf_id = leaf_id(payload_bytes);

// Generate a DM story ID
let dm_id = dm_story_id(&member_a_id, &member_b_id);
```

`dm_story_id` is symmetric — `dm_story_id(A, B) == dm_story_id(B, A)`.

### Sharing in a Realm

```rust
// Default sharing (Revocable access)
let artifact_id = realm.share_artifact(
    "report.pdf",
    bytes,
    "application/pdf",
).await?;

// Share with specific access mode
let artifact_id = realm.share_artifact_with_mode(
    "report.pdf",
    bytes,
    "application/pdf",
    AccessMode::Permanent,
).await?;

// Granular sharing to specific members
let artifact_id = realm.share_artifact_granular(
    "report.pdf",
    bytes,
    "application/pdf",
    &[member_a, member_b],
    AccessMode::Timed(expiry_timestamp),
).await?;
```

### Viewing Shared Artifacts

```rust
let artifacts: Vec<HomeArtifactEntry> = realm.artifacts_view().await?;
```

### Downloading

```rust
let download: ArtifactDownload = realm.download(artifact_id).await?;

// Track progress
use futures::StreamExt;
let mut progress = download.progress();
while let Some(p) = progress.next().await {
    println!("{}%", p.percent());
}

// Wait for completion
let file_path: PathBuf = download.finish().await?;

// Or cancel
download.cancel();
```

### DownloadProgress

```rust
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
}

impl DownloadProgress {
    pub fn percent(&self) -> f32;
    pub fn is_complete(&self) -> bool;
}
```

### ArtifactDownload Handle

The download handle provides:

| Method | Returns | Description |
|--------|---------|-------------|
| `artifact_id()` | `&ArtifactId` | The artifact being downloaded |
| `name()` | `&str` | Human-readable name |
| `current_progress()` | `DownloadProgress` | Current state snapshot |
| `progress()` | `Stream<DownloadProgress>` | Live progress updates |
| `finish()` | `Result<PathBuf>` | Wait for completion, return file path |
| `cancel()` | `()` | Cancel the download |
| `is_cancelled()` | `bool` | Check cancellation state |

---

## Access Control

Access control governs who can read, modify, and redistribute artifacts.

### AccessMode

```rust
pub enum AccessMode {
    Revocable,                // Owner can revoke at any time
    Permanent,                // Cannot be revoked once granted
    Timed(u64),               // Auto-expires at the given Unix timestamp
    Transfer,                 // Ownership transfer — grantee becomes new owner
}
```

### Granting Access

```rust
home.grant_access(artifact_id, grantee_member_id, AccessMode::Revocable).await?;
```

Grants are stored in the `ArtifactIndex` as part of the `HomeArtifactEntry`.

### Revoking Access

```rust
home.revoke_access(artifact_id, grantee_member_id).await?;
```

Only works for `Revocable` and expired `Timed` grants. Attempting to revoke a `Permanent` grant returns `RevokeError`.

### Recalling Artifacts

```rust
home.recall(artifact_id).await?;
```

Recall is stronger than revoke — it removes the artifact entirely and notifies all grantees that the artifact has been recalled. This triggers `Content::ArtifactRecalled` messages in the relevant realms.

### Transferring Ownership

```rust
home.transfer(artifact_id, new_owner_member_id).await?;
```

Transfers the artifact to a new owner. After transfer, the original owner loses all access and the new owner can manage grants.

### Querying Access

```rust
let grantees: Vec<(MemberId, AccessMode)> = home.shared_with(artifact_id).await?;
```

### Error Types

```rust
pub enum GrantError {
    AlreadyGranted,
    ArtifactNotFound,
    NotOwner,
    InvalidMode(String),
}

pub enum RevokeError {
    NotGranted,
    ArtifactNotFound,
    NotOwner,
    PermanentGrant,
}

pub enum TransferError {
    ArtifactNotFound,
    NotOwner,
    AlreadySelf,
}
```

### ArtifactProvenance

Every artifact tracks its origin:

```rust
pub struct ArtifactProvenance {
    pub provenance_type: ProvenanceType,
    pub original_author: MemberId,
    pub timestamp: u64,
}

pub enum ProvenanceType {
    Original,       // Created by the author
    Received,       // Received via sharing
    Imported,       // Imported from external source
}
```

### ArtifactStatus

```rust
pub enum ArtifactStatus {
    Active,
    Recalled,
    Expired,
    Transferred,
}
```

---

## Tree Composition

Tree composition lets you organize artifacts into hierarchical structures with inherited access control.

### Attaching Children

```rust
// Attach multiple children at once
home.attach_children(parent_tree_id, &[child_a_id, child_b_id]).await?;

// Attach a single child
home.attach_child(parent_tree_id, child_id).await?;
```

When a child is attached to a parent, it inherits the parent's access grants. If the parent is shared with Alice, Alice automatically gets access to all children.

### Detaching

```rust
// Detach all children
home.detach_all_children(parent_tree_id).await?;

// Detach a specific child
home.detach_child(parent_tree_id, child_id).await?;
```

Detaching removes the parent-child relationship and revokes inherited access.

### ArtifactIndex Tree Operations

The `ArtifactIndex` provides low-level tree operations:

```rust
let ancestors: Vec<ArtifactId> = index.ancestors(artifact_id);
let descendants: Vec<ArtifactId> = index.descendants(artifact_id);
let depth: usize = index.depth(artifact_id);
let subtree_size: usize = index.subtree_size(artifact_id);
```

### Inherited Access

```rust
let has_access: bool = index.has_access_with_inheritance(artifact_id, member_id);
```

This walks up the tree checking each ancestor's grants. If any ancestor grants access to the member, the function returns `true`.

### Cascade Operations

```rust
index.recall_cascade(artifact_id);
```

Recalls an artifact and all its descendants. This is used when a parent Tree is recalled — all children are automatically recalled too.

### HomeArtifactEntry

```rust
pub struct HomeArtifactEntry {
    pub id: ArtifactId,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub created_at: u64,
    pub encrypted_key: Option<EncryptedArtifactKey>,
    pub status: ArtifactStatus,
    pub grants: Vec<AccessGrant>,
    pub provenance: ArtifactProvenance,
    pub parent: Option<ArtifactId>,
}
```

The `parent` field is the link in the tree structure. It points to the parent Tree artifact's ID (if any).

---

## Artifact Sync

The `ArtifactSyncRegistry` manages automatic P2P synchronization of artifacts. When you grant access to an artifact, the sync system automatically creates the networking infrastructure (gossip topics, download tasks) needed for the grantee to receive the data.

### How It Works

Each artifact gets its own deterministic interface ID and key seed:

```rust
pub fn artifact_interface_id(artifact_id: &ArtifactId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"indras:artifact-sync:");
    hasher.update(&artifact_id.as_bytes());
    *hasher.finalize().as_bytes()
}

pub fn artifact_key_seed(artifact_id: &ArtifactId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"indras:artifact-key:");
    hasher.update(&artifact_id.as_bytes());
    *hasher.finalize().as_bytes()
}
```

### Reconciliation

```rust
let registry = ArtifactSyncRegistry::new(node.clone());

// Reconcile: ensure sync is running for all granted artifacts
registry.reconcile(&artifact_index).await?;
```

`reconcile` compares the current set of active sync interfaces against what the artifact index says should be syncing. It creates new interfaces for new grants and tears down interfaces for revoked grants.

### Ensure and Teardown

```rust
// Manually ensure sync for one artifact
registry.ensure(artifact_id).await?;

// Tear down sync for one artifact
registry.teardown(artifact_id).await?;
```

### Startup Recovery

On startup, the sync registry runs `reconcile` to restore any sync sessions that were active before shutdown. This ensures that artifact downloads resume where they left off.

### Store-and-Forward Sync Primitives

Under the hood, artifact sync uses three primitives from `indras-sync`:

#### ArtifactDocument

A per-tree Automerge document wrapping `AutoCommit`. Stores artifact metadata, references, grants, and key-value metadata with CRDT semantics:

```rust
use indras_sync::ArtifactDocument;
use indras_artifacts::{ArtifactId, TreeType};

// Create a new document
let mut doc = ArtifactDocument::new(&artifact_id, &steward_id, &TreeType::Story, now);

// Add references to child artifacts
doc.append_ref(&child_id, 0, Some("chapter-1"));

// Manage grants
doc.add_grant(&grant);
doc.remove_grant(&grantee_id);

// Arbitrary metadata
doc.set_metadata("mime", b"image/png");

// Lifecycle status
doc.set_status(&ArtifactStatus::Recalled { recalled_at: now });

// Persistence
let bytes = doc.save();
let loaded = ArtifactDocument::load(&bytes)?;
```

For bootstrapping from a received sync payload, use `ArtifactDocument::empty()` — the schema is populated by `load_incremental()`.

> **Gotcha**: Never cache Automerge `ObjId`s — they go stale after sync/merge. `ArtifactDocument` re-looks up object IDs on every access.

#### HeadTracker

Tracks the last-known Automerge `ChangeHash` heads for each `(ArtifactId, peer)` pair. Enables incremental sync:

```rust
use indras_sync::HeadTracker;

let mut tracker = HeadTracker::new();

// Record what a peer has seen
tracker.update(&artifact_id, &peer_id, current_heads);

// Query — empty slice means full sync needed
let known: &[ChangeHash] = tracker.get(&artifact_id, &peer_id);

// Cleanup
tracker.remove_peer(&peer_id);
tracker.remove_artifact(&artifact_id);

// Persistence (postcard)
let bytes = tracker.save()?;
let loaded = HeadTracker::load(&bytes)?;
```

#### RawSync

Stateless functions for producing and consuming sync payloads:

```rust
use indras_sync::{RawSync, ArtifactSyncPayload};

// Sender: build a payload for one recipient
let payload: ArtifactSyncPayload = RawSync::prepare_payload(
    &mut doc, &tracker, &artifact_id, &recipient_id,
);
// → transport the payload (gossip, relay, store-and-forward)

// Receiver: apply the payload
RawSync::apply_payload(&mut doc, &mut tracker, payload, &sender_id)?;

// Broadcast to all audience members (skips self)
let payloads = RawSync::broadcast_payloads(
    &mut doc, &tracker, &artifact_id, &audience, &self_id,
);
```

`prepare_payload` checks the tracker for what the recipient already has and sends only the delta. `apply_payload` is idempotent — duplicate changes are silently ignored by Automerge.

---

## Chat Messages

The chat message system builds on top of Documents to provide editable, versioned messages.

### EditableChatMessage

```rust
pub struct EditableChatMessage {
    pub id: ChatMessageId,
    pub author: MemberId,
    pub author_name: String,
    pub content: EditableMessageType,
    pub created_at: u64,
    pub edited_at: Option<u64>,
    pub deleted: bool,
    pub versions: Vec<ChatMessageVersion>,
    pub reply_to: Option<ChatMessageId>,
}
```

### ChatMessageId and Versions

```rust
pub type ChatMessageId = String;

pub struct ChatMessageVersion {
    pub content: EditableMessageType,
    pub timestamp: u64,
    pub author: MemberId,
}
```

Every edit creates a new version. The `versions` vec is the complete edit history. The current content is always the latest version.

### EditableMessageType

```rust
pub enum EditableMessageType {
    Text,
    ProofSubmitted {
        quest_id: String,
        artifact_id: String,
    },
    ProofFolderSubmitted {
        quest_id: String,
        folder_id: String,
    },
    BlessingGiven {
        quest_id: String,
        claimant: String,
    },
    ArtifactRecalled {
        artifact_hash: String,
        shared_at: u64,
    },
    Image {
        mime_type: String,
        inline_data: Option<String>,   // base64 for small images (<2MB)
        artifact_hash: Option<String>, // hash ref for large images
        filename: Option<String>,
        dimensions: Option<(u32, u32)>,
        alt_text: Option<String>,
    },
    Gallery {
        folder_id: String,
        title: Option<String>,
        items: Vec<GalleryItem>,
    },
}
```

Note: `Text` is a unit variant — the text content is stored in the `EditableChatMessage.current_content` field, not in the enum. The `Image` variant supports both inline base64 data (for small images) and artifact hash references (for large images). `ProofFolderSubmitted` is new — it represents a proof folder artifact submitted for a quest.

### RealmChatDocument

```rust
pub struct RealmChatDocument {
    messages: Vec<EditableChatMessage>,
}
```

This is a CRDT document that stores all chat messages for a realm. Methods:

```rust
let doc: Document<RealmChatDocument> = realm.document("chat").await?;

doc.update(|chat| {
    chat.add_message(message);
}).await?;

doc.update(|chat| {
    chat.edit_message(message_id, new_content, author, timestamp);
}).await?;

doc.update(|chat| {
    chat.delete_message(message_id, author);
}).await?;
```

---

## Read Tracking

The read tracker keeps per-member read positions for unread message counting.

### ReadTrackerDocument

```rust
pub struct ReadTrackerDocument {
    // Maps MemberId (hex) -> last read sequence number
    read_positions: HashMap<String, u64>,
}
```

This is a CRDT document with LWW (Last Writer Wins) semantics — each member can only update their own read position.

### Marking as Read

```rust
realm.mark_read().await?;
```

Sets your read position to the current latest sequence number.

### Checking Unread Count

```rust
let unread: usize = realm.unread_count().await?;
let last_read: u64 = realm.last_read_seq().await?;
```

`unread_count` returns the number of messages with sequence numbers higher than your last read position.

---

## Realm Aliases

Realm aliases let members give custom nicknames to realms. These are CRDT-synchronized across all members.

### RealmAlias

```rust
pub struct RealmAlias {
    pub realm_id: RealmId,
    pub alias: String,
    pub set_by: MemberId,
    pub set_at: u64,
}
```

Aliases are limited to `MAX_ALIAS_LENGTH` (77 characters) and support full Unicode.

### Setting Aliases

```rust
realm.set_alias("The Cool Project").await?;
```

### Getting Aliases

```rust
let alias: Option<RealmAlias> = realm.get_alias().await?;
let display: String = realm.alias().await; // Returns alias or realm name
```

`alias()` returns the alias if set, otherwise falls back to the realm name.

### Clearing Aliases

```rust
realm.clear_alias().await?;
```

### RealmAliasDocument

The underlying CRDT document that stores aliases for all realms. Members can see each other's aliases but each member controls their own.

---

## Encryption

### Per-Artifact Encryption

Every artifact can be encrypted with its own key:

```rust
pub const ARTIFACT_KEY_SIZE: usize = 32;
pub type ArtifactKey = [u8; ARTIFACT_KEY_SIZE];
```

### EncryptedArtifactKey

```rust
pub struct EncryptedArtifactKey {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}
```

The artifact key is encrypted with the owner's master key and stored alongside the artifact metadata in the `HomeArtifactEntry`. When granting access, the key is re-encrypted for the grantee's public key.

---

## Identity Export & Import

### Exporting

```rust
let backup: IdentityBackup = network.export_identity().await?;
// Serialize and save the backup
```

### Importing

```rust
let backup: IdentityBackup = /* deserialize from file */;
network.import_identity(backup).await?;
```

`IdentityBackup` contains the cryptographic keypair and enough metadata to reconstruct the identity on a new device.

### Artifact Recovery

For recovering artifacts after an identity restore:

```rust
pub struct ArtifactRecoveryRequest {
    pub requesting_member: MemberId,
    pub artifact_ids: Vec<ArtifactId>,
}

pub struct ArtifactRecoveryResponse {
    pub provider: MemberId,
    pub artifacts: Vec<RecoverableArtifact>,
}

pub struct RecoverableArtifact {
    pub id: ArtifactId,
    pub name: String,
    pub encrypted_key: EncryptedArtifactKey,
}

pub struct RecoveryManifest {
    pub artifacts: Vec<RecoverableArtifact>,
    pub timestamp: u64,
}
```

Recovery works by sending `Content::RecoveryRequest` messages to realm members, who respond with `Content::RecoveryManifest` containing the artifacts they can provide.

---

## Blocking

```rust
network.block_contact(member_id).await?;
```

Blocking a contact:
1. Sets their sentiment to `-1` in the contacts document
2. Leaves all shared realms (DM realms with that peer)
3. Prevents future connection attempts from that peer

---

## World View

WorldView provides a diagnostic snapshot of the entire network state.

### Building a World View

```rust
let world_view: WorldView = network.save_world_view().await?;
```

### WorldView Structure

```rust
pub struct WorldView {
    pub timestamp: String,
    pub node: NodeInfo,
    pub interfaces: Vec<InterfaceInfo>,
    pub peers: Vec<PeerViewInfo>,
    pub transport: TransportInfo,
}

pub struct NodeInfo {
    pub display_name: Option<String>,
    pub iroh_public_key: String,
    pub member_id: String,
    pub endpoint_addr: Option<String>,
    pub data_dir: String,
}

pub struct InterfaceInfo {
    pub id: String,
    pub name: Option<String>,
    pub event_count: u64,
    pub member_count: u32,
    pub encrypted: bool,
    pub created_at_millis: i64,
    pub last_activity_millis: i64,
    pub members: Vec<MemberViewInfo>,
    pub documents: Vec<DocumentInfo>,
}

pub struct DocumentInfo {
    pub name: String,
    pub data_size_bytes: usize,
    pub chat_message_count: Option<usize>,
    pub recent_message_ids: Option<Vec<String>>,
}

pub struct MemberViewInfo {
    pub peer_id: String,
    pub role: String,
    pub active: bool,
    pub joined_at_millis: i64,
}

pub struct PeerViewInfo {
    pub peer_id: String,
    pub display_name: Option<String>,
    pub first_seen_millis: i64,
    pub last_seen_millis: i64,
    pub message_count: u64,
    pub trusted: bool,
    pub connected: bool,
    pub has_pq_encapsulation_key: bool,
    pub has_pq_verifying_key: bool,
}

pub struct TransportInfo {
    pub connected_peers: Vec<String>,
    pub discovered_peers: Vec<String>,
    pub active_realm_topics: Vec<String>,
}
```

### Saving to File

```rust
let world_view = WorldView::build(&network).await;
world_view.save(Path::new("/path/to/world_view.json"))?;

// Or via the network convenience method:
network.save_world_view().await?;
```

The output is JSON, designed to be read by diagnostic tools or the dashboard UI. Comparing world view files from different nodes reveals sync discrepancies — each interface includes per-document data sizes and recent chat message IDs for diffing.

---

## Error Handling

All fallible operations return `Result<T, IndraError>`.

### IndraError

```rust
pub enum IndraError {
    // Invite errors
    InvalidInvite(String),
    InviteExpired,

    // Realm errors
    RealmFull,
    RemovedFromRealm,
    RealmNotFound(String),

    // Document errors
    DocumentNotFound(String),

    // Connection errors
    NotConnected,
    NotStarted,
    AlreadyStarted,
    Timeout(String),

    // Permission errors
    NotMember,
    InvalidOperation(String),

    // Infrastructure errors
    Network(String),
    Storage(String),
    Sync(String),
    Crypto(String),
    Serialization(String),
    Io(std::io::Error),
    Config(String),
    Schema(String),

    // Artifact errors
    Artifact(String),

    // Authentication errors
    StoryAuth(String),
}
```

Most variants carry a `String` with diagnostic details. `Io` wraps a standard `std::io::Error`.

### The Result Type

```rust
pub type Result<T> = std::result::Result<T, IndraError>;
```

Imported via the prelude:

```rust
use indras_network::prelude::*;

async fn do_stuff() -> Result<()> {
    // ...
    Ok(())
}
```

---

## Escape Hatches

For advanced users who need direct access to the underlying infrastructure:

```rust
use indras_network::escape::*;
```

### Available Re-exports

| Module | Types | Description |
|--------|-------|-------------|
| `indras_core` | `NodeId`, `InterfaceId`, `PeerId` | Core identifiers |
| `indras_node` | `Node`, `NodeConfig`, `NodeEvent` | The P2P node |
| `indras_sync` | `SyncEngine`, `SyncConfig`, `SyncEvent` | CRDT sync engine |
| `indras_transport` | `Transport`, `Connection` | Network transport layer |
| `indras_storage` | `Storage`, `StorageConfig` | Persistent storage |
| `indras_crypto` | `Keypair`, `PublicKey`, `Signature` | Cryptographic primitives |
| `indras_messaging` | `MessageBroker`, `Subscription` | Message routing |

### From IndrasNetwork

```rust
let node = network.node();           // Arc<Node>
let storage = network.storage();     // Arc<Storage>
let config = network.config();       // &NetworkConfig
```

### From Realm

```rust
let node = realm.node();             // &Node
let node_arc = realm.node_arc();     // Arc<Node>
```

### Additional Types

The escape module also re-exports:

- `RealmConfig` — Low-level realm configuration
- `Encryption` — Encryption primitives
- `OfflineDelivery` — Offline message queuing

---

## Re-exported Types

`indras-network` re-exports the entire `indras-artifacts` crate for ergonomic imports:

```rust
// Blanket re-export
pub use indras_artifacts;

// Specific type re-exports for convenience
pub use indras_artifacts::{
    Artifact, LeafArtifact, TreeArtifact, ArtifactRef,
    LeafType, TreeType,
    BlessingRecord, StewardshipRecord,
    AttentionLog, AttentionSwitchEvent, AttentionValue, compute_heat,
    PeerEntry, PeerRegistry, MutualPeering,
    ArtifactStore, PayloadStore, AttentionStore,
    InMemoryArtifactStore, InMemoryAttentionStore, InMemoryPayloadStore,
    IntegrityResult,
    Vault, Story, Exchange, Request,
    VaultError,
    compute_token_value,
};
```

This means consumers only need one dependency — `indras-network` — to access both the high-level SDK and the full artifact domain model.

### Domain Types

| Type | Description |
|------|-------------|
| `Artifact` | Union type for all artifacts |
| `LeafArtifact` | Immutable content (files, notes) |
| `TreeArtifact` | Container for other artifacts |
| `ArtifactRef` | Lightweight reference to an artifact |
| `LeafType` | Enum: Note, File, Image, Quest, Proof, etc. |
| `TreeType` | Enum: Vault, Story, Exchange, Request, etc. |

### Vault Types

| Type | Description |
|------|-------------|
| `Vault` | A personal vault (top-level container) |
| `Story` | A narrative thread of artifacts |
| `Exchange` | A trade or gift between peers |
| `Request` | A request for artifacts or actions |
| `VaultError` | Error type for vault operations |

### Attention Economy

| Type | Description |
|------|-------------|
| `AttentionLog` | Record of attention given to artifacts |
| `AttentionSwitchEvent` | Individual focus change event |
| `AttentionValue` | Computed value of attention |
| `compute_heat` | Calculate artifact "heat" from attention |
| `compute_token_value` | Derive token value from attention data |

### Peer Registry

| Type | Description |
|------|-------------|
| `PeerEntry` | A peer's information |
| `PeerRegistry` | Collection of known peers |
| `MutualPeering` | Bidirectional peer relationship |

### Stores

| Type | Description |
|------|-------------|
| `ArtifactStore` | Trait for artifact persistence |
| `PayloadStore` | Trait for payload (binary content) persistence |
| `AttentionStore` | Trait for attention data persistence |
| `InMemoryArtifactStore` | In-memory implementation |
| `InMemoryPayloadStore` | In-memory implementation |
| `InMemoryAttentionStore` | In-memory implementation |

### Integrity

| Type | Description |
|------|-------------|
| `IntegrityResult` | Result of artifact integrity verification |
| `BlessingRecord` | Record of a blessing given to an artifact |
| `StewardshipRecord` | Record of stewardship responsibility |

---

## The Prelude

Import everything commonly needed with one line:

```rust
use indras_network::prelude::*;
```

The prelude includes:

```rust
pub use crate::{
    ArtifactDownload, ArtifactIndex, HomeArtifactEntry,
    ContactsRealm, Content, Document, DocumentSchema,
    EditableChatMessage, GlobalEvent, HomeRealm, IdentityBackup,
    IdentityCode, IndraError, IndrasNetwork, InviteCode, Member,
    MemberEvent, MemberInfo, Message, Preset, Realm, RealmAlias,
    RealmAliasDocument, RealmChatDocument, RealmId, Result,
};

pub use futures::StreamExt;
```

Note that `futures::StreamExt` is included so you can iterate async streams without a separate import.

---

## Global Events

Subscribe to network-wide events:

```rust
let mut events = network.events();
while let Some(event) = events.next().await {
    match event {
        GlobalEvent::RealmJoined(realm) => { /* ... */ },
        GlobalEvent::RealmLeft(realm_id) => { /* ... */ },
        GlobalEvent::PeerConnected(member_id) => { /* ... */ },
        GlobalEvent::PeerDisconnected(member_id) => { /* ... */ },
        GlobalEvent::ArtifactReceived(artifact_id) => { /* ... */ },
        GlobalEvent::IdentityUpdated => { /* ... */ },
    }
}
```

Global events fire for things that happen across realms — new connections, realm joins/leaves, incoming artifacts, and identity changes.

---

## Putting It All Together

Here's a complete example of a chat application:

```rust
use indras_network::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize with chat preset
    let network = IndrasNetwork::preset(Preset::Chat)
        .data_dir("~/.mychat")
        .display_name("Alice")
        .build()
        .await?;

    network.start().await?;

    // Create a realm
    let realm = network.create_realm("Book Club").await?;
    println!("Share this invite: {}", realm.invite_code().unwrap());

    // Set a friendly alias
    realm.set_alias("Book Club (2026)").await?;

    // Share a document
    let home = network.home_realm().await?;
    let artifact_id = home.share_file("/path/to/reading-list.pdf").await?;

    // Share the artifact in the realm
    realm.send(Content::artifact(artifact_id, "reading-list.pdf")).await?;

    // Listen for messages and member events concurrently
    let realm_clone = realm.clone();
    tokio::spawn(async move {
        let mut members = realm_clone.member_events();
        while let Some(event) = members.next().await {
            match event {
                MemberEvent::Joined(m) => println!("{} joined!", m.display_name),
                MemberEvent::Left(id) => println!("Member left: {:?}", id),
                _ => {}
            }
        }
    });

    let mut messages = realm.messages();
    while let Some(msg) = messages.next().await {
        match &msg.content {
            Content::Text(text) => {
                println!("{}: {}", msg.sender_name, text);
            }
            Content::Image { filename, .. } => {
                println!("{} sent an image: {}", msg.sender_name, filename);
            }
            Content::Artifact { name, .. } => {
                println!("{} shared: {}", msg.sender_name, name);
            }
            _ => {}
        }
    }

    network.stop().await?;
    Ok(())
}
```

---

*This guide covers every public module, type, and method in `indras-network` v0.1. The crate is the single import surface for building on Indra's Network — it re-exports the full `indras-artifacts` domain model so you never need to depend on lower-level crates directly.*
