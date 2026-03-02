# indras-chat

Standalone Telegram-style P2P chat desktop app (binary) and embeddable library. Provides
a full sidebar-based conversation UI with message bubbles, input bar, contact management,
and a setup flow. Designed to run standalone or be embedded in other apps (e.g.,
`indras-workspace`) via the `bridge` module. Chat-specific CSS is exported as `CHAT_CSS`.

## Module Map

```
src/
  lib.rs          — pub mod bridge, components, state; pub const CHAT_CSS
  main.rs         — standalone Dioxus desktop launch entry point
  state.rs        — unified app state: conversation list, active contact, message history
  style.css       — chat-specific CSS (bubble layout, sidebar, input bar)
  bridge.rs       — network + sync-engine bridge; spawns background tasks for
                    message send/receive, contact resolution, realm subscription

  components/
    mod.rs        — re-exports all components
    app.rs        — App root; reads state, switches between setup and main UI
    sidebar.rs    — Sidebar — contact/conversation list with unread badges
    chat_view.rs  — ChatView — message history pane for the active conversation
    message_bubble.rs — MessageBubble — single message with sender, timestamp, body
    message_input.rs  — MessageInput — text input + send button with Enter-key handler
    contact_add.rs    — ContactAdd — form for pasting a peer invite link
    setup.rs          — Setup — first-launch screen: node init, display name
```

## Key Types

- `bridge` module — the async layer between Dioxus signals and the network stack;
  consumers call `bridge::init()` to connect to `indras-network` and `indras-sync-engine`,
  then write to state signals from event callbacks
- `state` module — holds conversation list, selected contact key, and per-conversation
  message `Vec`; all fields are Dioxus signals for reactive updates

## Key Patterns

- Embedding: host apps add `indras-chat` as a dependency, call `CHAT_CSS` into their
  `with_custom_head`, and render the `App` component (or individual components) inside
  their own layout. The bridge must be initialised once via `bridge::init()`.
- Message send path: `MessageInput` → state signal write → bridge picks up via watch →
  `indras-sync-engine` appends to realm document → sync distributes to peers
- Contact add: user pastes an invite link into `ContactAdd`; bridge resolves the public
  key via `indras-network` pkarr lookup, then opens a direct realm with that peer
- Standalone launch: `main.rs` initialises `tracing-subscriber` then launches Dioxus
  desktop with both `CHAT_CSS` and any injected theme CSS

## Dependencies

| Crate | Role |
|---|---|
| `dioxus` (0.7, desktop) | UI framework |
| `indras-network` | Peer resolution, connection management |
| `indras-sync-engine` | Message storage and sync via realm documents |
| `indras-crypto` | Key formatting, invite link encoding |
| `tokio` | Async runtime (multi-thread) |
| `serde` / `serde_json` | Message serialisation |
| `chrono` | Message timestamp formatting |
| `hex` | Public key hex display |
| `futures` | Stream combinators in bridge |

## Testing

No automated tests. Run `cargo run -p indras-chat` standalone to verify the full flow:
setup screen → display name → contact add (paste a peer invite) → message send/receive.
For embedding, build `indras-workspace` and confirm the embedded chat panel appears and
messages round-trip between two running instances.
