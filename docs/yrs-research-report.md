# Yrs (Rust Yjs Implementation) - Comprehensive Research Report

**Research Date:** February 14, 2026
**Latest Version:** yrs 0.25.0, y-sync 0.4.0

---

## Executive Summary

Yrs is a high-performance Rust implementation of the Yjs CRDT (Conflict-free Replicated Data Type) framework. It provides shared data types (Text, Array, Map, XmlFragment) that automatically sync and merge without conflicts, making it ideal for collaborative applications. Yrs maintains binary protocol compatibility with Yjs (JavaScript) and offers excellent performance characteristics.

**Key Cargo Dependencies:**
```toml
[dependencies]
yrs = "0.25.0"           # Core CRDT implementation
y-sync = "0.4.0"         # Sync protocol + Awareness
yrs-warp = "0.9.0"       # Optional: Warp WebSocket integration
# yrs-axum also available for Axum framework
```

**Feature Flags:**
- `sync` - Enable sync protocol support (multi-threaded)
- `weak` - Enable weak references support
- `default` - No additional features by default

---

## 1. Core Types and API Signatures

### 1.1 Document (Doc)

The `Doc` is the top-level container for all shared types. Each document tracks causality independently.

```rust
use yrs::*;

// Create a new document with auto-generated ClientID
let doc = Doc::new();

// Create with specific ClientID (must be unique per peer!)
let doc = Doc::with_client_id(42);

// Get or insert root shared types
let text: TextRef = doc.get_or_insert_text("article");
let array: ArrayRef = doc.get_or_insert_array("items");
let map: MapRef = doc.get_or_insert_map("metadata");
let xml: XmlFragmentRef = doc.get_or_insert_xml_fragment("document");
```

**Critical:** Each active peer MUST have a unique ClientID. Sharing IDs causes corruption.

### 1.2 Transactions

All operations happen within transaction scope. Yrs provides two transaction types:

#### Read-Only Transaction
```rust
let txn = doc.transact();  // Multiple can coexist
let content = text.get_string(&txn);
```

#### Read-Write Transaction (TransactionMut)
```rust
let mut txn = doc.transact_mut();  // Requires exclusive access
text.insert(&mut txn, 0, "Hello");
text.insert(&mut txn, 5, " world");
// Auto-commits when dropped
```

**Key Points:**
- Read-write transactions require exclusive `Doc` access
- Changes are batched and compressed automatically
- Commit happens on drop (RAII pattern)
- Cannot have multiple `transact_mut()` simultaneously

### 1.3 Shared Types

All types have two representations:
- **Integrated types** (TextRef, ArrayRef, etc.) - attached to Doc
- **Preliminary types** (TextPrelim, ArrayPrelim, etc.) - not yet integrated

#### Text (TextRef)

Collaborative rich text with formatting attributes.

```rust
let text = doc.get_or_insert_text("name");

{
    let mut txn = doc.transact_mut();

    // Insert text at position
    text.insert(&mut txn, 0, "Hello");

    // Insert with formatting
    text.insert_with_attributes(
        &mut txn,
        5,
        " world",
        Attrs::from([("bold", true)])
    );

    // Delete range
    text.remove_range(&mut txn, 0, 5);

    // Get string content
    let content = text.get_string(&txn);
}
```

**Cursor Positioning with StickyIndex:**
```rust
// Create sticky cursor that maintains position across edits
let cursor = text.sticky_index(&mut txn, index, Assoc::After)?;

// Later, get updated position
let current_index = cursor.get_offset(&txn)?.index;
```

#### Array (ArrayRef)

Indexable sequence of values.

```rust
let array = doc.get_or_insert_array("items");

{
    let mut txn = doc.transact_mut();

    // Push values
    array.push_back(&mut txn, "item1");
    array.push_back(&mut txn, 42);
    array.push_back(&mut txn, true);

    // Insert at index
    array.insert(&mut txn, 1, "inserted");

    // Delete range
    array.remove_range(&mut txn, 0, 2);

    // Get length
    let len = array.len(&txn);

    // Iterate
    for value in array.iter(&txn) {
        // Process value
    }
}
```

#### Map (MapRef)

String-keyed map with Last-Write-Wins semantics.

```rust
let map = doc.get_or_insert_map("metadata");

{
    let mut txn = doc.transact_mut();

    // Set values
    map.insert(&mut txn, "title", "My Document");
    map.insert(&mut txn, "version", 1);

    // Get value
    if let Some(value) = map.get(&txn, "title") {
        // Process value
    }

    // Remove key
    map.remove(&mut txn, "title");

    // Iterate entries
    for (key, value) in map.iter(&txn) {
        println!("{}: {:?}", key, value);
    }
}
```

**Conflict Resolution:** When multiple peers set the same key, the value from the client with the highest ClientID and clock value wins.

#### XML Types (XmlFragmentRef, XmlElementRef, XmlTextRef)

XML node representations for structured documents.

```rust
let xml = doc.get_or_insert_xml_fragment("document");

{
    let mut txn = doc.transact_mut();

    // Create element
    let elem = XmlElementPrelim::empty("div");
    xml.insert(&mut txn, 0, elem);

    // XML types support:
    // - Attributes (for XmlElement)
    // - Child nodes (nested structure)
    // - Text content (XmlText)
}
```

**Note:** All XML types use the same underlying `Branch` type and can work as both indexed sequences and maps.

---

## 2. Sync Protocol

Yrs uses a 2-step delta-state CRDT sync protocol with highly optimized binary encoding.

### 2.1 StateVector

A `StateVector` is a compact representation of all known blocks inserted into a document. It acts as a logical timestamp describing which updates have been observed.

```rust
use yrs::updates::encoder::Encode;

let doc1 = Doc::new();
let doc2 = Doc::new();

// Get state vector from doc2
let state_vector = {
    let txn = doc2.transact();
    txn.state_vector()
};

// Encode as bytes for transmission
let mut encoder = Vec::new();
state_vector.encode(&mut encoder);
```

### 2.2 Update Encoding/Decoding

Yrs supports two encoding versions:

- **V1** (default): Optimal for individual updates, smaller payload
- **V2**: Optimized when encoding multiple updates together (e.g., full document state)

#### Differential Update (Pull Model)

```rust
use yrs::updates::decoder::Decode;

// Doc1 has changes, Doc2 wants to sync

// Step 1: Doc2 sends its state vector to Doc1
let state_vec = {
    let txn = doc2.transact();
    txn.state_vector()
};

// Step 2: Doc1 calculates diff based on Doc2's state
let update = {
    let txn = doc1.transact();
    txn.encode_diff_v1(&state_vec)
};

// Step 3: Doc2 applies the update
{
    let mut txn = doc2.transact_mut();
    let decoded = Update::decode_v1(&update)?;
    txn.apply_update(decoded)?;
}
```

#### Full State Encoding

```rust
// Encode entire document state (no state vector)
let full_state = {
    let txn = doc1.transact();
    txn.encode_state_as_update_v1()
};

// Later, restore from storage
{
    let mut txn = doc2.transact_mut();
    let update = Update::decode_v1(&full_state)?;
    txn.apply_update(update)?;
}
```

#### Push Model (Event-Based)

```rust
use yrs::Subscription;

// Subscribe to updates on doc1
let _subscription = doc1.observe_update_v1(|txn, event| {
    // event.update contains the binary update
    let update_bytes = event.update.as_ref();

    // Send update_bytes to remote peers over network
    broadcast_to_peers(update_bytes);
});

// When receiving updates from peers:
fn handle_remote_update(update_bytes: &[u8]) {
    let mut txn = doc2.transact_mut();
    let update = Update::decode_v1(update_bytes).unwrap();
    txn.apply_update(update).unwrap();
}
```

### 2.3 Sync Protocol Steps

The standard sync flow:

1. **SyncStep1:** Peer A sends its StateVector to Peer B
2. **SyncStep2:** Peer B responds with differential Update containing only what A is missing
3. **Apply:** Peer A applies the Update to merge changes

This is implemented in the `y-sync` crate.

---

## 3. Awareness Protocol

Awareness provides lightweight presence/cursor tracking WITHOUT the overhead of CRDT conflict resolution. It uses a clock-based timeout mechanism (30 seconds by default).

### 3.1 Awareness API

```rust
use y_sync::awareness::Awareness;
use serde_json::json;

let doc = Doc::new();
let mut awareness = Awareness::new(doc);

// Set local state (user presence info)
awareness.set_local_state(json!({
    "user": {
        "name": "Zephyr",
        "color": "#ff6b6b"
    },
    "cursor": {
        "line": 10,
        "column": 5
    }
}));

// Update individual field
awareness.set_local_state_field("cursor", json!({
    "line": 11,
    "column": 8
}));

// Get all client states
let states = awareness.get_states();
for (client_id, state) in states {
    println!("Client {}: {:?}", client_id, state);
}

// Get local state
if let Some(local) = awareness.get_local_state() {
    println!("My state: {:?}", local);
}

// Listen to awareness changes
awareness.on_update(|event, awareness| {
    // event.added: Vec<ClientID>
    // event.updated: Vec<ClientID>
    // event.removed: Vec<ClientID>
    println!("Awareness updated: {:?}", event);
});

// Indicate offline (set state to null)
awareness.clean_local_state();

// Destroy awareness
awareness.destroy();
```

### 3.2 Key Differences from Y.Doc

| Feature | Y.Doc | Awareness |
|---------|-------|-----------|
| **Purpose** | Document content | Presence/cursors |
| **State Vectors** | Yes (minimal sync) | No (always full state) |
| **Persistence** | Permanent | Ephemeral (session-only) |
| **Conflict Resolution** | CRDT merge | Last-write-wins |
| **Timeout** | None | 30-second auto-cleanup |
| **Network Cost** | Incremental updates | Full state broadcasts |

**Important:** Awareness state must be broadcast regularly (< 30s intervals) to prevent remote timeout and removal.

---

## 4. Persistence

### 4.1 Save Document State

```rust
use std::fs::File;
use std::io::Write;

// Encode full document state
let state_bytes = {
    let txn = doc.transact();
    txn.encode_state_as_update_v1()
};

// Save to file
let mut file = File::create("document.yrs")?;
file.write_all(&state_bytes)?;
```

### 4.2 Load Document State

```rust
use std::fs;

// Load from file
let state_bytes = fs::read("document.yrs")?;

// Apply to new document
let doc = Doc::new();
{
    let mut txn = doc.transact_mut();
    let update = Update::decode_v1(&state_bytes)?;
    txn.apply_update(update)?;
}
```

### 4.3 Incremental Storage Pattern

For databases that store individual updates:

```rust
use std::sync::{Arc, Mutex};

let updates_log = Arc::new(Mutex::new(Vec::new()));

// Store each update as it happens
let updates_clone = updates_log.clone();
let _sub = doc.observe_update_v1(move |_txn, event| {
    let mut log = updates_clone.lock().unwrap();
    log.push(event.update.to_vec());
});

// Later, restore by applying all updates in order
let doc2 = Doc::new();
{
    let mut txn = doc2.transact_mut();
    let log = updates_log.lock().unwrap();
    for update_bytes in log.iter() {
        let update = Update::decode_v1(update_bytes).unwrap();
        txn.apply_update(update).unwrap();
    }
}
```

### 4.4 Snapshots (Requires `skip_gc: true`)

```rust
use yrs::*;

let options = Options {
    skip_gc: true,  // Preserve history
    ..Default::default()
};
let doc = Doc::with_options(options);

// Create snapshot at current state
let snapshot = {
    let txn = doc.transact();
    txn.snapshot()
};

// Later, encode state at that snapshot
let state_at_snapshot = {
    let txn = doc.transact();
    let mut encoder = Vec::new();
    txn.encode_state_from_snapshot(&snapshot, &mut encoder)?;
    encoder
};
```

---

## 5. Observation and Subscriptions

### 5.1 Observer Event Order

Observers fire in this sequence during transaction commit:

1. **Collection observers** (TextRef::observe, ArrayRef::observe, etc.)
2. **Deep observers** (bubble up from nested types)
3. **After-transaction callbacks**
4. **Cleanup callbacks**
5. **Update callbacks** (Doc::observe_update_v1/v2)
6. **Subdocument callbacks**

### 5.2 Type-Level Observers

```rust
// Observe text changes
let sub: Subscription = text.observe(move |txn, event| {
    for delta in event.delta(txn) {
        match delta {
            Delta::Inserted(value, attrs) => {
                println!("Inserted: {:?} with {:?}", value, attrs);
            }
            Delta::Deleted(len) => {
                println!("Deleted {} chars", len);
            }
            Delta::Retain(len, attrs) => {
                println!("Retained {} chars, attrs: {:?}", len, attrs);
            }
        }
    }
});

// Unsubscribe
drop(sub);
```

### 5.3 Deep Observers

Deep observers fire when nested collections change:

```rust
// Observe map and all nested changes
let sub = map.observe_deep(move |txn, events| {
    for event in events {
        println!("Deep change at path: {:?}", event.path());
    }
});
```

### 5.4 Document Update Observers

```rust
// Observe all document changes (V1 encoding)
let sub = doc.observe_update_v1(|txn, event| {
    println!("Update size: {} bytes", event.update.len());
    // Send event.update to remote peers
});

// V2 encoding (better for batched updates)
let sub = doc.observe_update_v2(|txn, event| {
    // Same as v1, but uses v2 encoding
});
```

### 5.5 Subscription Type

All observers return a `Subscription` handle:

```rust
let sub = text.observe(|txn, event| { /* ... */ });

// Unsubscribe by dropping
drop(sub);

// Or let it go out of scope
{
    let _sub = text.observe(|txn, event| { /* ... */ });
} // Unsubscribed here
```

---

## 6. Undo/Redo Management

```rust
use yrs::undo::{UndoManager, Options};

// Create undo manager for specific types
let mut mgr = UndoManager::new(&doc, &text);

// With custom options
let opts = Options {
    capture_timeout_millis: 500,  // Merge edits within 500ms
    ..Default::default()
};
let mut mgr = UndoManager::with_options(&doc, &text, opts);

// Track changes
{
    let mut txn = doc.transact_mut();
    text.insert(&mut txn, 0, "Hello");
}

// Undo
mgr.undo().unwrap();

// Redo
mgr.redo().unwrap();

// Check if can undo/redo
if mgr.can_undo() {
    mgr.undo().unwrap();
}
```

---

## 7. Integration Patterns

### 7.1 AppFlowy's Strongly-Typed Wrapper Pattern

AppFlowy-Collab demonstrates best practices:

```rust
// AppFlowy's pattern: Domain-specific wrappers over Yrs
pub struct DocumentCollab {
    doc: Arc<Doc>,
    root: MapRef,
}

impl DocumentCollab {
    pub fn new() -> Self {
        let doc = Arc::new(Doc::new());
        let root = doc.get_or_insert_map("document");
        Self { doc, root }
    }

    pub fn set_title(&self, title: &str) {
        let mut txn = self.doc.transact_mut();
        self.root.insert(&mut txn, "title", title);
    }

    pub fn get_title(&self) -> Option<String> {
        let txn = self.doc.transact();
        self.root.get(&txn, "title")
            .and_then(|v| v.cast::<String>())
    }
}
```

**Benefits:**
- Type-safe API
- Schema validation
- Transaction helpers
- Domain-specific methods
- Easier testing

### 7.2 WebSocket Server Integration (yrs-warp)

```rust
use yrs::Doc;
use y_sync::awareness::Awareness;
use yrs_warp::broadcast::BroadcastGroup;
use std::sync::{Arc, RwLock};

#[tokio::main]
async fn main() {
    // Shared document for all peers
    let awareness = Arc::new(RwLock::new(Awareness::new(Doc::new())));

    // Broadcast group with 32-update buffer
    let bcast = Arc::new(
        BroadcastGroup::new(awareness.clone(), 32).await
    );

    // WebSocket route
    let route = warp::path("ws")
        .and(warp::ws())
        .map(move |ws: warp::ws::Ws| {
            let bcast = bcast.clone();
            ws.on_upgrade(move |socket| async move {
                // Handle WebSocket connection
                bcast.subscribe(socket).await;
            })
        });

    warp::serve(route).run(([127, 0, 0, 1], 3030)).await;
}
```

**BroadcastGroup Features:**
- Automatic update distribution
- Awareness sync
- Buffered messages
- Client management

### 7.3 Axum Integration (yrs-axum)

Similar pattern with Axum framework - check `yrs-axum` examples.

### 7.4 Custom Protocol Extension

```rust
use y_sync::sync::{Protocol, DefaultProtocol, Message};

struct CustomProtocol {
    default: DefaultProtocol,
}

impl Protocol for CustomProtocol {
    fn missing_handle(
        &mut self,
        awareness: &mut Awareness,
        tag: u8,
        data: Vec<u8>,
    ) -> Option<Message> {
        // Echo unknown messages back
        Some(Message::Custom { tag, data })
    }

    // Delegate other methods to DefaultProtocol
    // ...
}
```

---

## 8. Architecture Insights

### 8.1 Block Store

The core of Yrs is a block store where each block contains:

- **ID:** Unique identifier (ClientID + clock)
- **Origin pointers:** Left/right positions at insertion
- **Current pointers:** Current position in linked list
- **Parent reference:** For nested collections
- **Key:** For map-like types (optional)
- **Content:** Payload (primitive or nested CRDT)

### 8.2 YATA Algorithm

Yrs uses YATA (Yet Another Transformation Approach) for conflict resolution:

- Prevents interleaving issues
- Consistent ordering based on block IDs and neighbors
- Universal strategy across all data types

### 8.3 Optimizations

**Block Squashing:**
- Consecutive blocks from same client merge
- Reduces metadata overhead
- Improves memory locality

**Block Splitting:**
- When inserting between elements in existing block
- Computational complexity independent of insert count

**Binary Encoding:**
- Varint encoding for integers
- Field deduplication (inferred from context)
- Bit flags for content types
- Delete sets use (start, length) pairs

---

## 9. Practical Examples

### 9.1 Complete Sync Example

```rust
use yrs::*;

fn main() {
    // Peer 1
    let doc1 = Doc::new();
    let text1 = doc1.get_or_insert_text("article");

    {
        let mut txn = doc1.transact_mut();
        text1.insert(&mut txn, 0, "Hello from peer 1");
    }

    // Peer 2 (separate machine/process)
    let doc2 = Doc::new();
    let text2 = doc2.get_or_insert_text("article");

    {
        let mut txn = doc2.transact_mut();
        text2.insert(&mut txn, 0, "Greetings from peer 2");
    }

    // Sync: Peer 2 -> Peer 1
    let state_vec_1 = {
        let txn = doc1.transact();
        txn.state_vector()
    };

    let update_from_2 = {
        let txn = doc2.transact();
        txn.encode_diff_v1(&state_vec_1)
    };

    {
        let mut txn = doc1.transact_mut();
        let update = Update::decode_v1(&update_from_2).unwrap();
        txn.apply_update(update).unwrap();
    }

    // Sync: Peer 1 -> Peer 2
    let state_vec_2 = {
        let txn = doc2.transact();
        txn.state_vector()
    };

    let update_from_1 = {
        let txn = doc1.transact();
        txn.encode_diff_v1(&state_vec_2)
    };

    {
        let mut txn = doc2.transact_mut();
        let update = Update::decode_v1(&update_from_1).unwrap();
        txn.apply_update(update).unwrap();
    }

    // Both docs now have merged content
    let content1 = {
        let txn = doc1.transact();
        text1.get_string(&txn)
    };

    let content2 = {
        let txn = doc2.transact();
        text2.get_string(&txn)
    };

    assert_eq!(content1, content2);
    println!("Merged: {}", content1);
}
```

### 9.2 Collaborative Map with Observers

```rust
use yrs::*;
use std::sync::Arc;

fn main() {
    let doc = Doc::new();
    let map = doc.get_or_insert_map("config");

    // Set up observer
    let _sub = map.observe(move |txn, event| {
        for (key, change) in event.keys(txn) {
            match change {
                EntryChange::Inserted(value) => {
                    println!("Key '{}' inserted: {:?}", key, value);
                }
                EntryChange::Updated(old, new) => {
                    println!("Key '{}' updated: {:?} -> {:?}", key, old, new);
                }
                EntryChange::Removed(old) => {
                    println!("Key '{}' removed: {:?}", key, old);
                }
            }
        }
    });

    // Make changes
    {
        let mut txn = doc.transact_mut();
        map.insert(&mut txn, "theme", "dark");
        map.insert(&mut txn, "font_size", 14);
        map.insert(&mut txn, "theme", "light");  // Update
    }

    // Observer fires after transaction commits
}
```

---

## 10. Common Patterns and Best Practices

### 10.1 ClientID Management

```rust
// Generate unique ClientID per peer
use rand::Rng;

let client_id = rand::thread_rng().gen::<u64>();
let doc = Doc::with_client_id(client_id);
```

**Never share ClientIDs between active peers!**

### 10.2 Error Handling

```rust
use yrs::Result;

fn sync_documents(doc1: &Doc, doc2: &Doc) -> Result<()> {
    let state_vec = {
        let txn = doc1.transact();
        txn.state_vector()
    };

    let update = {
        let txn = doc2.transact();
        txn.encode_diff_v1(&state_vec)
    };

    let mut txn = doc1.transact_mut();
    let decoded = Update::decode_v1(&update)?;
    txn.apply_update(decoded)?;

    Ok(())
}
```

### 10.3 Multi-threaded Access

Enable the `sync` feature flag:

```toml
[dependencies]
yrs = { version = "0.25.0", features = ["sync"] }
```

Then use `Arc` for shared access:

```rust
use std::sync::Arc;
use std::thread;

let doc = Arc::new(Doc::new());

let doc_clone = doc.clone();
let handle = thread::spawn(move || {
    // Safe concurrent read access
    let txn = doc_clone.transact();
    // ...
});
```

### 10.4 Weak References

Enable weak references with feature flag:

```toml
[dependencies]
yrs = { version = "0.25.0", features = ["weak"] }
```

Useful for avoiding circular references in nested structures.

---

## 11. Performance Characteristics

### 11.1 Benchmarks

Yrs is designed for high performance:

- **Encoding/decoding:** Optimized binary format (lib0)
- **Memory:** Block squashing reduces overhead
- **CPU:** Insert complexity independent of document size (block splitting)

Check `yrs/benches/` in the repository for detailed benchmarks.

### 11.2 Memory Management

```rust
// Enable garbage collection (default)
let doc = Doc::new();

// Disable GC for snapshots/history
let doc = Doc::with_options(Options {
    skip_gc: true,
    ..Default::default()
});
```

**Note:** Disabling GC preserves all history but increases memory usage.

---

## 12. Comparison with Automerge

| Feature | Yrs | Automerge |
|---------|-----|-----------|
| **Algorithm** | YATA | RGA/OpSet |
| **Encoding** | lib0 (binary, compact) | Columnar (optimized) |
| **Language** | Rust | Rust |
| **JavaScript Compat** | Yjs (binary compatible) | @automerge/automerge |
| **Transactions** | Explicit (RAII) | Explicit |
| **Persistence** | StateVector + Updates | Save/load entire doc |
| **Sync** | 2-step delta | SyncState exchange |
| **Text CRDT** | YATA | RGA |
| **Awareness** | Separate protocol | Not built-in |
| **Maturity** | Very mature (Yjs heritage) | Mature |
| **Performance** | Excellent | Excellent |

**Key Differences:**
- Yrs has tighter Yjs ecosystem integration (compatible with JavaScript Yjs)
- Automerge has richer Rust API (cursor handling, marks, etc.)
- Yrs uses explicit transactions; Automerge similar pattern
- Both have strong performance, different optimization focuses

---

## 13. Resources and Links

### Official Documentation
- [Yrs crate docs](https://docs.rs/yrs/0.25.0) - Rust API reference
- [y-sync crate docs](https://docs.rs/y-sync/0.4.0) - Sync protocol
- [GitHub: y-crdt/y-crdt](https://github.com/y-crdt/y-crdt) - Main repository
- [Yjs documentation](https://docs.yjs.dev/) - JavaScript equivalent (protocol compatible)

### Architecture Deep Dives
- [Yrs Architecture](https://www.bartoszsypytkowski.com/yrs-architecture/) - Detailed internals
- [Yjs Internals](https://github.com/yjs/yjs/blob/main/INTERNALS.md) - Algorithm explanation

### Integration Examples
- [AppFlowy-Collab](https://github.com/AppFlowy-IO/AppFlowy-Collab) - Production usage
- [yrs-warp examples](https://github.com/y-crdt/yrs-warp) - WebSocket server
- [GitHub: y-crdt examples](https://github.com/y-crdt/y-crdt/tree/main/yrs) - Basic examples

### Related Crates
- `yrs-warp` (0.9.0) - Warp WebSocket integration
- `yrs-axum` - Axum framework integration
- `yrs-tokio` - Tokio async utilities
- `ywasm` - WebAssembly bindings
- `yffi` - C foreign function interface

### Community
- [Yjs Community Forum](https://discuss.yjs.dev/) - Q&A and discussions
- [Open Collective](https://opencollective.com/y-collective) - Funding and project info

---

## 14. Summary for IndrasNetwork Migration

### Recommended Approach

1. **Wrapper Layer:** Create strongly-typed domain objects (follow AppFlowy pattern)
2. **Persistence:** Use `observe_update_v1` + incremental storage
3. **Sync:** Leverage `y-sync` protocol with custom extensions as needed
4. **Awareness:** Use for presence/cursors, NOT document data
5. **Testing:** Start with single-peer scenarios, then add sync

### Key Cargo.toml

```toml
[dependencies]
yrs = { version = "0.25.0", features = ["sync"] }
y-sync = "0.4.0"
serde_json = "1.0"  # For awareness state serialization

# Optional: WebSocket server
yrs-warp = "0.9.0"
# OR
# yrs-axum = "latest"
```

### Migration Checklist

- [ ] Identify Automerge usage patterns in IndrasNetwork
- [ ] Map Automerge types to Yrs equivalents (Text, Map, Array)
- [ ] Design strongly-typed wrapper layer
- [ ] Implement persistence strategy (incremental or snapshot)
- [ ] Add sync protocol integration (if networked)
- [ ] Port observers/subscriptions to Yrs patterns
- [ ] Test CRDT merge behavior with concurrent edits
- [ ] Benchmark performance vs. Automerge

### Code Translation Patterns

| Automerge Pattern | Yrs Equivalent |
|-------------------|----------------|
| `doc.put()` | `map.insert(&mut txn, key, value)` |
| `doc.get()` | `map.get(&txn, key)` |
| `doc.splice_text()` | `text.insert(&mut txn, idx, str)` + `text.remove_range()` |
| `doc.commit()` | Transaction auto-commits on drop |
| `doc.save()` | `txn.encode_state_as_update_v1()` |
| `doc.load()` | `txn.apply_update(update)` |
| `doc.get_changes()` | `txn.encode_diff_v1(&state_vec)` |

---

**Research completed:** February 14, 2026
**Report version:** 1.0
**Next steps:** Begin migration planning for IndrasNetwork
