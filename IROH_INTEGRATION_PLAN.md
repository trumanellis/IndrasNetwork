# Indra's Network: Developer Experience Transformation

**Project:** Indra's Network
**Version:** 2.0 - The Developer Experience Release
**Created:** 2026-01-24

---

## The Problem

Indra's Network v1.0 is **powerful infrastructure** with 13+ crates, sophisticated DTN routing, CRDT sync, and post-quantum cryptography. But it's not **usable** for the average developer building a P2P app.

Compare the current experience:

```rust
// Current: Developer must understand and compose 7+ crates
use indras_transport::Transport;
use indras_routing::StoreForwardRouter;
use indras_storage::CompositeStorage;
use indras_sync::NInterface;
use indras_messaging::MessageClient;
use indras_crypto::InterfaceKey;
use indras_gossip::GossipHandle;

// Then wire everything together manually...
```

To what developers expect:

```rust
// What developers want
let app = Indra::new("~/.myapp").await?;
let realm = app.join("realm-invite-code").await?;

realm.send("Hello, world!").await?;

while let Some(msg) = realm.messages().next().await {
    println!("{}: {}", msg.sender, msg.text);
}
```

**This plan transforms Indra's Network from infrastructure into a developer-friendly SyncEngine.**

---

## Design Principles

### 1. Progressive Disclosure
- Simple things should be simple
- Complex things should be possible
- Complexity is opt-in, not mandatory

### 2. Zero Configuration Required
- Discovery, routing, encryption, persistence all work out of the box
- Sensible defaults everywhere
- Override when you need to

### 3. Familiar Patterns
- Reactive streams (not callback hell)
- Async/await throughout
- Looks like what JS/TS developers expect

### 4. Cross-Platform First
- Same API on native, browser, mobile
- Write once, run anywhere
- Platform differences hidden behind abstractions

### 5. Observable & Debuggable
- Everything is inspectable
- DevTools for development
- Metrics and tracing for production

---

## Phase 1: The Indra SyncEngine

**Priority:** Critical
**Goal:** One crate to rule them all

### New Crate: `indra` (the SyncEngine)

This is the **only crate most developers will ever need**.

```
crates/indra/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Re-exports and prelude
│   ├── app.rs           # IndraApp - the main entry point
│   ├── realm.rs         # Realm - simplified N-peer interface
│   ├── document.rs      # Document - reactive CRDT wrapper
│   ├── message.rs       # Message types
│   ├── artifact.rs      # Artifact sharing abstraction
│   ├── identity.rs      # Identity management
│   ├── invite.rs        # Invite codes and joining
│   ├── config.rs        # Configuration with defaults
│   └── stream.rs        # Reactive stream utilities
```

### The IndraApp

```rust
use indra::prelude::*;

// Minimal setup - everything configured automatically
let app = Indra::new("~/.myapp").await?;

// Or with configuration
let app = Indra::builder()
    .data_dir("~/.myapp")
    .display_name("Alice")
    .relay_servers(vec!["relay.example.com"])
    .build()
    .await?;

// Your identity (persistent across restarts)
println!("I am: {}", app.id());  // indra:abc123...
```

### Realms (Elevated N-Peer Interfaces)

**"Realm"** is the developer-facing abstraction over N-peer interfaces. It's a collaborative space where members share messages, documents, and artifacts.

```rust
// Create a new realm
let realm = app.create_realm("Project Alpha").await?;
println!("Invite others: {}", realm.invite_code());

// Join an existing realm
let realm = app.join("indra:realm:abc123...").await?;

// Realm has everything you need
realm.name()           // "Project Alpha"
realm.members()        // Stream<Member>
realm.messages()       // Stream<Message>
realm.documents()      // Access to shared documents
realm.artifacts()      // Access to shared artifacts
realm.presence()       // Who's online right now
```

### Reactive Streams (Not Callbacks)

Everything returns **async streams** that work naturally with modern UI frameworks:

```rust
// Messages as a stream
let mut messages = realm.messages();
while let Some(msg) = messages.next().await {
    match msg.content {
        Content::Text(text) => println!("{}: {}", msg.sender.name, text),
        Content::Artifact(artifact) => println!("{} shared: {}", msg.sender.name, artifact.name),
        Content::Reaction(emoji, target) => println!("{} reacted {}", msg.sender.name, emoji),
    }
}

// Presence as a stream
let mut presence = realm.presence();
while let Some(event) = presence.next().await {
    match event {
        PresenceEvent::Online(member) => println!("{} is online", member.name),
        PresenceEvent::Offline(member) => println!("{} went offline", member.name),
        PresenceEvent::Typing(member) => println!("{} is typing...", member.name),
    }
}

// Combine streams for UI updates
let updates = futures::stream::select(
    realm.messages().map(Update::Message),
    realm.presence().map(Update::Presence),
);
```

### Documents (Reactive CRDTs)

Documents wrap Automerge with a reactive, type-safe API:

```rust
// Get or create a document
let doc = realm.document::<TodoList>("todos").await?;

// Read current state
let todos: &TodoList = doc.read();
println!("You have {} todos", todos.items.len());

// Make changes (automatically synced to all members)
doc.update(|todos| {
    todos.items.push(Todo {
        text: "Buy milk".into(),
        done: false,
    });
}).await?;

// Subscribe to changes from other members
let mut changes = doc.changes();
while let Some(change) = changes.next().await {
    println!("Document updated by {}", change.author.name);
    // UI can re-render here
}
```

### Artifact Sharing

Simple artifact sharing with automatic chunking, resumable transfers, and progress:

```rust
// Share an artifact
let artifact = realm.share_artifact("/path/to/document.pdf").await?;
println!("Shared: {} ({})", artifact.name, artifact.ticket());

// Download a shared artifact
let download = realm.download(&artifact).await?;

// With progress
let mut progress = download.progress();
while let Some(p) = progress.next().await {
    println!("{}% complete", p.percent());
}

let path = download.finish().await?;
println!("Saved to: {}", path);
```

### Identity & Invites

```rust
// Your identity persists across restarts
let me = app.identity();
println!("Public key: {}", me.public_key());
println!("Display name: {}", me.display_name());

// Update your profile
app.set_display_name("Alice Smith").await?;
app.set_avatar("/path/to/avatar.png").await?;

// Invite codes are human-shareable
let code = realm.invite_code();
// => "indra:realm:7xK9mN2pQ..."

// Or generate a QR code
let qr = realm.invite_qr();
qr.save_png("/path/to/invite.png")?;
```

### Error Handling

Clear, actionable errors:

```rust
match app.join(invite_code).await {
    Ok(realm) => { /* success */ }
    Err(JoinError::InvalidCode) => println!("That invite code isn't valid"),
    Err(JoinError::Expired) => println!("That invite has expired"),
    Err(JoinError::RealmFull) => println!("That realm is at capacity"),
    Err(JoinError::Banned) => println!("You've been removed from this realm"),
    Err(JoinError::Network(e)) => println!("Network issue: {}", e),
}
```

### Configuration Presets

```rust
// For chat apps
let app = Indra::preset(Preset::Chat)
    .data_dir("~/.mychat")
    .build().await?;

// For collaborative documents
let app = Indra::preset(Preset::Collaboration)
    .data_dir("~/.mydocs")
    .build().await?;

// For IoT / embedded
let app = Indra::preset(Preset::IoT)
    .data_dir("/var/lib/mydevice")
    .build().await?;

// For offline-first / DTN
let app = Indra::preset(Preset::OfflineFirst)
    .data_dir("~/.myapp")
    .build().await?;
```

---

## Phase 2: Cross-Platform Runtime

**Priority:** High
**Goal:** Same API everywhere

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      indra (SyncEngine)                                │
│  Realms • Documents • Artifacts • Messages • Identity           │
├─────────────────────────────────────────────────────────────────┤
│                   indra-runtime                                 │
│  Platform abstraction layer                                     │
├──────────────┬──────────────┬──────────────┬───────────────────┤
│    Native    │    WASM      │   Mobile     │    Embedded       │
│   (Tokio)    │  (Browser)   │  (iOS/And)   │    (no_std)       │
├──────────────┼──────────────┼──────────────┼───────────────────┤
│ File system  │ IndexedDB    │ SQLite       │ Flash storage     │
│ iroh native  │ iroh WASM    │ iroh mobile  │ Custom transport  │
│ OS keychain  │ WebCrypto    │ Keychain     │ Secure element    │
└──────────────┴──────────────┴──────────────┴───────────────────┘
```

### New Crate: `indra-runtime`

Platform abstraction that the SyncEngine builds on:

```rust
// indra-runtime/src/lib.rs

/// Platform-specific implementations
pub trait Runtime: Send + Sync + 'static {
    type Storage: Storage;
    type Transport: Transport;
    type Crypto: Crypto;
    type Timer: Timer;

    fn storage(&self) -> &Self::Storage;
    fn transport(&self) -> &Self::Transport;
    fn crypto(&self) -> &Self::Crypto;
    fn spawn<F: Future>(&self, future: F);
}

// Implementations for each platform
#[cfg(not(target_arch = "wasm32"))]
pub type DefaultRuntime = native::NativeRuntime;

#[cfg(target_arch = "wasm32")]
pub type DefaultRuntime = wasm::WasmRuntime;
```

### Browser Package: `@indra/sdk`

First-class TypeScript/JavaScript support:

```typescript
// npm install @indra/sdk

import { Indra, Realm, Document } from '@indra/sdk';

// Same API as Rust!
const app = await Indra.create({ persist: 'myapp' });
const realm = await app.join('indra:realm:abc123...');

// Reactive streams become AsyncIterables
for await (const msg of realm.messages()) {
  console.log(`${msg.sender.name}: ${msg.content.text}`);
}

// Or use callbacks for framework integration
realm.messages().subscribe(msg => {
  // React setState, Vue ref update, etc.
});

// Documents with TypeScript types
interface TodoList {
  items: { text: string; done: boolean }[];
}

const doc = await realm.document<TodoList>('todos');
doc.update(todos => {
  todos.items.push({ text: 'Buy milk', done: false });
});

// Subscribe to changes
doc.subscribe(todos => {
  renderTodos(todos.items);
});
```

### Framework Integrations

#### React Hooks: `@indra/react`

```typescript
import { useIndra, useRealm, useDocument, useMessages } from '@indra/react';

function ChatRealm({ realmCode }) {
  const realm = useRealm(realmCode);
  const messages = useMessages(realm);
  const presence = usePresence(realm);

  return (
    <div>
      <MemberList members={presence.online} />
      <MessageList messages={messages} />
      <MessageInput onSend={msg => realm.send(msg)} />
    </div>
  );
}

function CollaborativeDoc({ realmCode, docId }) {
  const realm = useRealm(realmCode);
  const [doc, updateDoc] = useDocument<TodoList>(realm, docId);

  const addTodo = (text: string) => {
    updateDoc(todos => {
      todos.items.push({ text, done: false });
    });
  };

  return <TodoList todos={doc.items} onAdd={addTodo} />;
}
```

#### Vue Composables: `@indra/vue`

```typescript
import { useIndra, useRealm, useDocument } from '@indra/vue';

const realm = useRealm('indra:realm:abc123...');
const messages = useMessages(realm);
const doc = useDocument<TodoList>(realm, 'todos');

// All are reactive refs that auto-update
watch(messages, (newMessages) => {
  console.log('New message!', newMessages[newMessages.length - 1]);
});
```

#### Svelte Stores: `@indra/svelte`

```typescript
import { indra, realm, document } from '@indra/svelte';

const realm = realm('indra:realm:abc123...');
const messages = realm.messages;  // Svelte store
const doc = document(realm, 'todos');  // Reactive document

// Use directly in templates
// {#each $messages as msg}
```

---

## Phase 3: Developer Tooling

**Priority:** High
**Goal:** Best-in-class developer experience

### CLI Tool: `indra`

```bash
# Install
cargo install indra-cli
# or
npm install -g @indra/cli

# Create a new project
indra new my-chat-app
indra new my-collab-app --template collaboration
indra new my-game --template game-lobby

# Development server with hot reload
indra dev

# Create a test network (3 nodes by default)
indra network start
indra network status
indra network stop

# Inspect a running node
indra inspect ~/.myapp
indra inspect ~/.myapp realms
indra inspect ~/.myapp realm <realm-id> messages
indra inspect ~/.myapp realm <realm-id> document <doc-id>

# Debug connectivity
indra debug connectivity
indra debug peer <peer-id>

# Generate invite codes
indra invite create ~/.myapp <realm-id>
indra invite decode <invite-code>
```

### DevTools (Browser Extension & Desktop App)

Visual debugging and inspection:

```
┌─────────────────────────────────────────────────────────────────┐
│  Indra DevTools                                   [Realms ▼]    │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────────────────────────┐  │
│  │ Realms          │  │ Realm: Project Alpha                │  │
│  │ ├─ Project Alpha│  │                                     │  │
│  │ ├─ Gaming Group │  │ Members (3 online, 2 offline)       │  │
│  │ └─ Family Chat  │  │ ├─ 🟢 Alice (you)                  │  │
│  │                 │  │ ├─ 🟢 Bob                          │  │
│  │ Identity        │  │ ├─ 🟢 Carol                        │  │
│  │ ├─ Name: Alice  │  │ ├─ 🔴 Dave (last seen 2h ago)      │  │
│  │ └─ Key: abc123  │  │ └─ 🔴 Eve (last seen 1d ago)       │  │
│  │                 │  │                                     │  │
│  │ Network         │  │ Documents                           │  │
│  │ ├─ 5 peers      │  │ ├─ todos (TodoList)                │  │
│  │ ├─ 12ms latency │  │ ├─ notes (TextDocument)            │  │
│  │ └─ 100% relay   │  │ └─ whiteboard (Canvas)             │  │
│  └─────────────────┘  │                                     │  │
│                       │ Messages: 1,234 total               │  │
│                       │ Artifacts: 23 shared (145 MB)       │  │
│                       └─────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────┤
│  Console │ Network │ Documents │ Storage │ Timeline            │
├─────────────────────────────────────────────────────────────────┤
│  [Document Inspector: todos]                                    │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ {                                                        │   │
│  │   "items": [                                             │   │
│  │     { "text": "Buy milk", "done": false },               │   │
│  │     { "text": "Call mom", "done": true }                 │   │
│  │   ]                                                      │   │
│  │ }                                                        │   │
│  └─────────────────────────────────────────────────────────┘   │
│  History: [v1] → [v2 by Bob] → [v3 by Alice] → [v4 by Carol]   │
└─────────────────────────────────────────────────────────────────┘
```

### Project Templates

```bash
indra new my-app --template <template>
```

Available templates:
- `chat` - Real-time chat application
- `collaboration` - Document collaboration (like Notion)
- `game-lobby` - Multiplayer game matchmaking
- `artifact-sharing` - P2P artifact sharing
- `iot-hub` - IoT device coordination
- `social` - Social network foundation

Each template includes:
- Working example code
- Tests
- Documentation
- CI/CD configuration

---

## Phase 4: HTTP Gateway & REST API

**Priority:** Medium
**Goal:** Legacy system integration

For systems that can't use the native SyncEngine, provide a REST API:

### Gateway Server

```bash
# Run the gateway
indra gateway --port 8080 --data-dir ~/.indra-gateway
```

### REST API

```yaml
# OpenAPI spec included
openapi: 3.0.0
info:
  title: Indra Gateway API
  version: 1.0.0

paths:
  /api/v1/realms:
    get:
      summary: List all realms
    post:
      summary: Create a new realm

  /api/v1/realms/{realmId}:
    get:
      summary: Get realm details

  /api/v1/realms/{realmId}/messages:
    get:
      summary: Get message history
    post:
      summary: Send a message

  /api/v1/realms/{realmId}/documents/{docId}:
    get:
      summary: Get document state
    patch:
      summary: Apply changes to document

  /api/v1/realms/{realmId}/artifacts:
    get:
      summary: List shared artifacts
    post:
      summary: Upload and share an artifact

  # WebSocket for real-time updates
  /api/v1/realms/{realmId}/ws:
    get:
      summary: WebSocket connection for real-time updates
```

### Use Cases
- Backend services written in Python, Go, etc.
- Legacy system integration
- Serverless functions
- Mobile apps without native SyncEngine

---

## Phase 5: Advanced Features (Escape Hatches)

For power users who need more control, expose the underlying infrastructure:

### Direct Crate Access

```rust
use indra::prelude::*;

let app = Indra::new("~/.myapp").await?;

// Access underlying components when needed
let transport = app.transport();   // indras-transport
let router = app.router();         // indras-routing
let storage = app.storage();       // indras-storage
let crypto = app.crypto();         // indras-crypto

// Custom protocol on top of Indra transport
transport.register_protocol("my-custom-protocol", handler).await?;
```

### Realm Configuration

```rust
let realm = app.create_realm_with_config(RealmConfig {
    name: "High Security Realm",

    // Encryption settings
    encryption: Encryption::PostQuantum,  // Use ML-KEM

    // Membership
    max_members: Some(10),
    invite_only: true,

    // Threshold signatures for admin operations
    admin_threshold: Some(Threshold::new(2, 3)),  // 2-of-3

    // DTN settings for offline members
    offline_delivery: OfflineDelivery::StoreAndForward {
        max_age: Duration::days(7),
        max_size: ByteSize::mb(100),
    },

    // Routing strategy
    routing: Routing::Prophet,  // For challenged networks
}).await?;
```

### Custom Document Types

```rust
use indra::document::{DocumentSchema, Validator};

// Define a schema for your document
#[derive(DocumentSchema)]
struct GameState {
    #[validate(range(0..=100))]
    player_health: u32,

    #[validate(length(max = 50))]
    player_name: String,

    position: (f32, f32),
    inventory: Vec<Item>,
}

// Schema is automatically enforced
let doc = realm.document::<GameState>("game").await?;

doc.update(|state| {
    state.player_health = 150;  // This will be clamped to 100
}).await?;
```

### FROST Threshold Operations

```rust
// Create a realm with threshold admin
let realm = app.create_realm_with_threshold("Treasury",
    members: vec![alice, bob, carol, dave],
    threshold: 3,  // 3-of-4 required
).await?;

// Admin operations require threshold signatures
let operation = realm.propose_admin_operation(
    AdminOp::RemoveMember(eve)
).await?;

// Other admins approve
// (happens automatically when they're online)
let result = operation.await?;  // Completes when threshold reached
```

### Plugin System

```rust
// Load a WASM plugin
app.load_plugin("./plugins/auto-translate.wasm").await?;

// Plugins can:
// - Process messages before display
// - Add custom document types
// - Implement custom protocols
// - Add new artifact type handlers
```

---

## Migration Path

### For Existing Users (v1.0 → v2.0)

The existing crates remain available and functional. The new SyncEngine is an **additional layer**, not a replacement.

```rust
// Old way still works
use indras_node::IndrasNode;
use indras_sync::NInterface;
// ... etc

// New way (recommended)
use indra::prelude::*;
```

### Gradual Adoption

1. New projects use `indra` SyncEngine
2. Existing projects can adopt incrementally
3. Power users can mix SyncEngine with direct crate access
4. Nothing is removed or broken

---

## Package Structure

### Rust Crates (crates.io)

| Crate | Description | Target Audience |
|-------|-------------|-----------------|
| `indra` | High-level SyncEngine (the main entry point) | App developers |
| `indra-runtime` | Platform abstraction layer | SyncEngine internals |
| `indra-cli` | Command-line tools | Developers |
| `indra-gateway` | REST API server | Backend integration |
| `indras-*` | Low-level infrastructure (existing) | Power users |

### JavaScript Packages (npm)

| Package | Description |
|---------|-------------|
| `@indra/sdk` | Core SyncEngine for browser/Node.js |
| `@indra/react` | React hooks |
| `@indra/vue` | Vue composables |
| `@indra/svelte` | Svelte stores |
| `@indra/cli` | CLI tools |

### Mobile (Future)

| Package | Platform |
|---------|----------|
| `IndraSyncEngine` | Swift package for iOS |
| `indra-android` | Android AAR |
| `@indra/react-native` | React Native bridge |

---

## Success Metrics

### Developer Experience

- [ ] "Hello World" in < 10 lines of code
- [ ] Working chat app in < 100 lines
- [ ] Time to first message: < 5 minutes for new developer
- [ ] Zero configuration required for basic usage
- [ ] TypeScript types are complete and accurate

### Documentation

- [ ] Quick start guide (< 5 minute read)
- [ ] Comprehensive API reference
- [ ] Tutorial for each template
- [ ] Video walkthrough
- [ ] Example gallery

### Community

- [ ] Active Discord/Matrix channel
- [ ] Weekly office hours
- [ ] Contribution guide
- [ ] Plugin showcase

---

## Implementation Phases

### Phase 1: SyncEngine Foundation (4-6 weeks)
1. Create `indra` crate with core abstractions
2. Implement `IndraApp`, `Realm`, `Document`, `Message`, `Artifact`
3. Reactive stream infrastructure
4. Configuration and presets
5. Comprehensive tests

### Phase 2: Cross-Platform (4-6 weeks)
1. Create `indra-runtime` abstraction
2. WASM compilation and browser runtime
3. `@indra/sdk` npm package
4. IndexedDB storage backend
5. Browser example apps

### Phase 3: Framework Integrations (2-3 weeks)
1. `@indra/react` hooks
2. `@indra/vue` composables
3. `@indra/svelte` stores
4. Framework-specific examples

### Phase 4: Developer Tools (3-4 weeks)
1. `indra-cli` with project scaffolding
2. DevTools browser extension
3. Project templates
4. Documentation site

### Phase 5: Gateway & Advanced (3-4 weeks)
1. `indra-gateway` REST API
2. WebSocket real-time API
3. Advanced configuration options
4. Plugin system foundation

---

## Appendix A: API Comparison

### Before (Infrastructure)

```rust
// 50+ lines to send a message

use indras_transport::Transport;
use indras_routing::{StoreForwardRouter, RoutingConfig};
use indras_storage::{CompositeStorage, StorageConfig};
use indras_sync::{NInterface, SyncConfig};
use indras_messaging::{MessageClient, Message, MessageContent};
use indras_crypto::{InterfaceKey, KeyStore};
use indras_gossip::GossipHandle;
use indras_node::IndrasNode;

async fn main() -> Result<()> {
    // Initialize storage
    let storage = CompositeStorage::new(StorageConfig {
        path: "~/.myapp".into(),
        ..Default::default()
    }).await?;

    // Initialize transport
    let transport = Transport::new(TransportConfig::default()).await?;

    // Initialize router
    let router = StoreForwardRouter::new(RoutingConfig::default());

    // Initialize crypto
    let keystore = KeyStore::load_or_create("~/.myapp/keys")?;

    // Create node
    let node = IndrasNode::builder()
        .storage(storage)
        .transport(transport)
        .router(router)
        .keystore(keystore)
        .build()
        .await?;

    // Create or join interface
    let interface_key = InterfaceKey::generate();
    let interface = NInterface::create(
        "my-realm",
        interface_key,
        SyncConfig::default(),
    ).await?;

    // Create message client
    let client = MessageClient::new(&node, &interface);

    // Finally, send a message
    client.send(MessageContent::Text("Hello!".into())).await?;

    Ok(())
}
```

### After (SyncEngine)

```rust
// 5 lines to send a message

use indra::prelude::*;

async fn main() -> Result<()> {
    let app = Indra::new("~/.myapp").await?;
    let realm = app.join("indra:realm:abc123...").await?;
    realm.send("Hello!").await?;
    Ok(())
}
```

---

## Appendix B: Naming Decisions

| Old Name | New Name | Rationale |
|----------|----------|-----------|
| N-peer Interface | Realm | Evokes a shared space; fits Indra mythology |
| Interface Key | Realm (internal) | Hidden from most users |
| Event Log | Message History | More intuitive |
| Automerge Document | Document | Focus on what, not how |
| File | Artifact | Distinctive; implies something crafted/shared |
| Gossip Topic | (internal) | Implementation detail |
| Store-and-Forward | Offline Delivery | Describes the benefit |
| DTN Bundle | (internal) | Implementation detail |

---

## Appendix C: Competitive Analysis

| Feature | Indra SyncEngine | Gun.js | Yjs | libp2p |
|---------|-----------|--------|-----|--------|
| Lines to hello world | ~5 | ~5 | ~10 | ~50 |
| TypeScript support | First-class | Yes | Yes | Partial |
| CRDT documents | Yes (Automerge) | Yes (custom) | Yes | No |
| Offline-first | Yes (DTN) | Partial | Partial | No |
| E2E encryption | Yes (default) | Optional | No | Optional |
| Post-quantum | Yes | No | No | No |
| Mobile support | Planned | Yes | Yes | Partial |
| Artifact sharing | Built-in | Plugin | No | Via protocols |

---

## Document History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-24 | 1.0 | Initial infrastructure plan |
| 2026-01-24 | 2.0 | Complete rewrite focused on developer experience |
