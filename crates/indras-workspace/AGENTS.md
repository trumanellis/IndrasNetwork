# indras-workspace

## Purpose

Desktop workspace application built on `indras-network` and `indras-sync-engine`. Provides a
collaborative P2P environment centered on the Intention lifecycle — creating, tracking, and
completing intentions with peers.

## Architecture

Dioxus desktop app. State is managed through reactive signals. Business logic lives in services.
UI is broken into components. The network and vault are accessed through a bridge layer.

```
src/
  main.rs          # App entry point, boot sequence
  lib.rs           # Module declarations
  components/      # Dioxus UI components (IntentionBoard, tabs, cards)
  state/           # Workspace state structs and signals
  services/        # Boot, polling, intention data, event handling
  bridge/          # Vault access and network integration
  scripting/       # Lua scripting for test automation (optional feature)
```

## Key Concepts

### IntentionBoard

The main dashboard. Organized around the Intention cycle — a user creates an intention, works
toward it, and completes or abandons it. Peers can observe and interact with each other's
intentions.

### Four Tabs

| Tab | Description |
|-----|-------------|
| My Intentions | Personal intention list with create/edit/complete actions |
| Community | Intentions shared by peers in the current realm |
| Tokens | Token balances and transfer UI |
| Chat | Peer messaging via `indras-chat` |

### Services Layer

- **Boot service**: Initializes the network node, opens the vault, joins realms
- **Polling service**: Periodically syncs state from the sync engine
- **Intention data service**: CRUD operations for intentions
- **Event service**: Handles incoming network events and updates signals

### Bridge Layer

- **Vault bridge**: Reads/writes artifacts to the local vault via `indras-artifacts`
- **Network bridge**: Sends and receives messages via `indras-network`

## Dependencies

| Crate | Role |
|-------|------|
| `dioxus` | Desktop UI framework (reactive signals, component model) |
| `indras-network` | P2P networking, realm membership, messaging |
| `indras-sync-engine` | Intention sync protocol and state machine |
| `indras-artifacts` | Vault storage for intentions and files |
| `indras-ui` | Shared UI primitives and design tokens |
| `indras-chat` | Chat tab implementation |
| `indras-crypto` | Key management and signing |
| `mlua` (optional) | Lua 5.4 scripting runtime |
| `tokio` | Async runtime (multi-thread) |

## State Flow

1. App boots → bridge initializes network node and vault
2. Boot service joins configured realms
3. Polling service fetches current intention state from sync engine
4. Signals update → Dioxus re-renders affected components
5. User actions → services write to vault/network → signals update

## Notes

- The `lua-scripting` feature is for test automation only; not enabled in production builds
- All async work goes through Tokio; Dioxus signals are updated from async tasks via spawn
