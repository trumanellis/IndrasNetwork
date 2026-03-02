# indras-messaging

High-level encrypted messaging client layered on top of the indras-gossip broadcast protocol.
Provides end-to-end encrypted message delivery within interfaces, in-memory message history
with rich querying, reply threading, multiple content types, and a versioned schema registry
for forward-compatible custom message types.

## Module Map

| Module | Role |
|---|---|
| `lib.rs` | Re-exports for all public types |
| `client.rs` | `MessagingClient` — create/join interfaces, send, reply, subscribe |
| `message.rs` | `Message`, `MessageContent`, `MessageId`, `MessageEnvelope`, `EncryptionMetadata` |
| `history.rs` | `MessageHistory`, `MessageFilter` — in-memory store with BTreeMap indexing |
| `schema.rs` | `SchemaRegistry`, `SchemaVersion`, `TypedContent`, `ContentValidator`, `SchemaMigration` |
| `error.rs` | `MessagingError`, `MessagingResult` |

## Key Types

- **`MessagingClient<I>`** — generic over `PeerIdentity`; holds gossip handle, joined interface
  map (`DashMap`), and shared `MessageHistory`
- **`Message<I>`** — `id: MessageId`, `sender: I`, `interface_id`, `content: MessageContent`,
  `timestamp: DateTime<Utc>`, `reply_to: Option<MessageId>`
- **`MessageId`** — `(interface_id, sequence: u64, nonce: [u8; 8])`; nonce ensures uniqueness
  even when sequence numbers collide across clients
- **`MessageContent`** — enum: `Text(String)`, `Binary { mime_type, data }`,
  `File { name, size, hash: [u8; 32] }`, `Reaction { target: MessageId, reaction }`, `System(String)`
- **`MessageHistory<I>`** — `BTreeMap<InterfaceId, BTreeMap<u64, Message<I>>>` guarded by
  `RwLock`; secondary `BTreeMap<MessageId, (InterfaceId, u64)>` for O(log n) ID lookup
- **`MessageFilter<I>`** — builder-style filter: interface, sender, since/until time, text_only,
  limit, offset
- **`SchemaVersion`** — `(major: u16, minor: u16)`; compatible = same major, receiver minor ≥
  content minor
- **`TypedContent`** — wraps `(content_type: String, schema_version, data: Vec<u8>, metadata: HashMap)`
- **`ContentValidator`** — validates built-in types (UTF-8 for text, `mime_type` metadata for
  binary, `filename`+`size` for file, `target_message_id` for reaction, valid JSON for
  custom_json); extensible with `register_validator`
- **`SchemaRegistry`** — tracks `ContentTypeInfo` and `SchemaMigration` entries; pre-populated
  with all `content_types::*` constants

## Key Patterns

**Creating and using a client:**
```rust
let client = MessagingClient::new(identity, Arc::new(gossip));
let (interface_id, key) = client.create_interface().await?;
client.send_text(&interface_id, "Hello").await?;
let mut rx = client.messages(); // broadcast::Receiver<Message<I>>
```

**Joining an existing interface:**
```rust
let interface_id = client.join_interface(key, bootstrap_peers).await?;
```

**Querying history:**
```rust
let msgs = client.history().query(
    &MessageFilter::new().interface(id).limit(50).text_only()
)?;
let recent = client.history().latest(id, 10)?;
let since_seq = client.history().since(id, last_seen_seq)?;
```

**Incoming message pipeline:** gossip `TopicReceiver` → filter own messages (already stored on
send) → `postcard::from_bytes::<Message<I>>` → `history.store` → `broadcast::Sender` publish.
The receiver task is spawned per interface inside `create_interface` / `join_interface`.

**Content type constants** live in `schema::content_types`: `TEXT`, `BINARY`, `FILE`,
`REACTION`, `SYSTEM`, `CUSTOM_JSON`, `CUSTOM_BINARY`. All built-in types start with
`"indras.message."`.

## Gotchas

- `MessagingClient` is not `Clone`. Share via `Arc<MessagingClient<I>>` if multiple owners
  need it.
- The `broadcast::Sender` channel has capacity 1024. Slow consumers will miss messages
  (broadcast semantics, not mpsc). Subscribe with `client.messages()` before sending to avoid
  losing the first message.
- `MessageHistory` is in-memory only. It does not persist across process restarts. For durable
  history wire it to indras-storage separately.
- `MessageHistory::with_limit` enforces per-interface maximums by evicting the **oldest**
  message when the limit is reached — not the globally oldest.
- `MessageId` nonces are random, so two messages with the same `(interface_id, sequence)` from
  different senders are correctly treated as distinct.
- `SchemaVersion::can_read` requires receiver minor >= content minor. A node on `1.0` cannot
  read `1.1` content. Plan schema bumps carefully.
- `ContentValidator` runs custom validators **after** built-in type validation. A custom
  validator registered for `TEXT` fires in addition to, not instead of, the UTF-8 check.
- `TypedContent::custom_json` uses `serde_json`, not `postcard`. The rest of the crate uses
  `postcard` for `Message` serialization — do not mix them up.

## Dependencies

Internal: `indras-core`, `indras-crypto`, `indras-transport`, `indras-routing`,
`indras-storage`, `indras-gossip`

External: `iroh`, `tokio`, `async-trait`, `serde`, `postcard`, `serde_json`, `chrono`,
`dashmap`, `rand`, `thiserror`, `tracing`

## Testing

Unit tests are inline in each module. Integration tests (requiring a real iroh endpoint and
gossip node) are noted in comments within `client.rs` and live in `tests/`. The schema and
history modules have thorough inline unit tests that do not require async.

```bash
cargo test -p indras-messaging
```
