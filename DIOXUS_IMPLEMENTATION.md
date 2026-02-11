# SyncEngine — Dioxus Implementation Guide

## Why Dioxus

The switch from Tauri to Dioxus eliminates the biggest architectural friction: the IPC boundary. In Tauri, the iroh networking layer (Rust) and the UI layer (JavaScript in a WebView) communicate across a serialization bridge. Every artifact state update, every attention event, every peer presence signal must be serialized, sent over IPC, and deserialized.

In Dioxus, everything is Rust. The iroh node and the UI share the same process. Artifact state flows from iroh documents directly into Dioxus signals. Attention switch events are written from the UI component that triggered them. No serialization boundary. No message passing overhead. The data model and the view are in the same language and the same memory space.

This matters especially for SyncEngine because the UI *is* the attention tracking system. Every navigation event must be captured and logged instantly. An IPC round-trip between "user clicked" and "attention switch event written" introduces latency and failure modes that don't need to exist.

---

## Renderer Choice

Dioxus 0.7 offers two desktop rendering paths:

**WebView (dioxus-desktop)** — Uses the system WebView (WebKit on macOS/Linux, WebView2 on Windows). Full CSS support including animations, transforms, filters. Mature and stable.

**Dioxus Native (dioxus-native / Blitz)** — GPU-rendered via WGPU using Servo's Stylo engine. No WebView dependency. Lighter binary (~12MB). CSS support is good but not complete — some advanced animations and filters may be missing. Still in alpha as of 0.7.

**Recommendation for SyncEngine:** Start with **WebView** for the spatial browser. The zoom transitions, glow effects, and CSS transforms that make the spatial metaphor work require mature CSS animation support. Blitz is not there yet. The same RSX components will work on either renderer — switching later requires changing a feature flag, not rewriting code:

```toml
[features]
default = []
desktop = ["dioxus/desktop"]    # WebView renderer
# desktop = ["dioxus/native"]   # Switch to Blitz when ready
web = ["dioxus/web"]
```

This also means the app can ship as a web app (WASM) from day one with the same codebase.

---

## Architecture Overview

```
┌──────────────────────────────────────────────────┐
│                  Dioxus App                       │
│                                                   │
│  ┌─────────────┐  ┌──────────────────────────┐   │
│  │  UI Layer    │  │  iroh Layer               │   │
│  │             │  │                            │   │
│  │  Components  │◄─── Signals ───►  iroh Node  │   │
│  │  (RSX)      │  │                 ├─ Blobs   │   │
│  │             │  │                 ├─ Docs    │   │
│  │  Router     │  │                 └─ Gossip  │   │
│  │  Hooks      │  │                            │   │
│  └─────────────┘  └──────────────────────────┘   │
│         │                    │                     │
│         ▼                    ▼                     │
│  ┌─────────────┐  ┌──────────────────────────┐   │
│  │  Attention   │  │  Local Store              │   │
│  │  Tracker     │──►  (SQLite / sled)          │   │
│  │             │  │  - Attention logs          │   │
│  │  Writes to   │  │  - Computed attention     │   │
│  │  iroh doc   │  │  - Peer registry          │   │
│  └─────────────┘  └──────────────────────────┘   │
└──────────────────────────────────────────────────┘
```

Two async runtimes coexist:
- **Dioxus's runtime** — manages the virtual DOM, signals, and component lifecycle.
- **Tokio** — runs the iroh node, handles network I/O, document sync.

They communicate via Dioxus signals and channels. iroh events (new document entries, blob arrivals, peer connections) are pushed into signals that the UI reactively renders.

---

## Core State: Signals

Dioxus uses signals for reactive state. The iroh layer pushes state into these signals; the UI reads them reactively.

```rust
use dioxus::prelude::*;

/// Where the player currently is in the artifact tree.
/// Changing this IS an attention switch event.
#[derive(Clone, PartialEq)]
struct NavigationState {
    current_artifact: ArtifactId,
    parent_stack: Vec<ArtifactId>,     // breadcrumb trail
    zoom_level: ZoomLevel,              // vault / tree / leaf
}

/// The resolved view of the current artifact space.
#[derive(Clone, PartialEq)]
struct ArtifactView {
    artifact: ArtifactMeta,             // id, type, steward, audience
    children: Vec<ChildEntry>,          // references with heat values
    peer_presence: Vec<PeerPresence>,   // who else is here
    my_stewardship: bool,               // am I the steward?
}

/// A child artifact as seen from the parent's space.
#[derive(Clone, PartialEq)]
struct ChildEntry {
    id: ArtifactId,
    artifact_type: ArtifactType,
    heat: f32,                          // 0.0 (cold) to 1.0 (hot)
    familiar: bool,                     // have I attended to this before?
    preview: Option<String>,            // text preview or thumbnail hash
}
```

---

## Component Architecture

### The Spatial Shell

The root component. Always rendered. Provides orientation context around the current artifact space.

```rust
#[component]
fn SpatialShell() -> Element {
    let nav = use_context::<Signal<NavigationState>>();
    let view = use_context::<Signal<ArtifactView>>();

    rsx! {
        div { class: "spatial-shell",
            // Breadcrumb: parent spaces receding behind current view
            BreadcrumbTrail { stack: nav.read().parent_stack.clone() }

            // The current artifact space — this is where the spatial grammar lives
            ArtifactSpace { view: view.read().clone() }

            // Ambient peer presence indicators
            PeerPresenceLayer { peers: view.read().peer_presence.clone() }

            // Steward controls (only visible if you're the steward)
            if view.read().my_stewardship {
                StewardControls { artifact_id: nav.read().current_artifact.clone() }
            }
        }
    }
}
```

### Artifact Space

The polymorphic renderer. Reads the artifact type and renders the appropriate spatial grammar.

```rust
#[component]
fn ArtifactSpace(view: ArtifactView) -> Element {
    match view.artifact.artifact_type {
        ArtifactType::Vault        => rsx! { VaultSpace { view } },
        ArtifactType::Conversation => rsx! { ConversationSpace { view } },
        ArtifactType::Gallery      => rsx! { GallerySpace { view } },
        ArtifactType::Document     => rsx! { DocumentSpace { view } },
        ArtifactType::Request      => rsx! { RequestSpace { view } },
        ArtifactType::Exchange     => rsx! { ExchangeSpace { view } },
        ArtifactType::Leaf(_)      => rsx! { LeafView { view } },
    }
}
```

### Spatial Grammar Components

Each artifact type gets its own spatial layout:

```rust
/// Conversation: a branching path of message nodes
#[component]
fn ConversationSpace(view: ArtifactView) -> Element {
    rsx! {
        div { class: "conversation-space",
            for child in view.children.iter() {
                MessageNode {
                    entry: child.clone(),
                    onclick: move |_| navigate_to(child.id.clone()),
                }
            }
        }
    }
}

/// Gallery: spatial field of image artifacts
#[component]
fn GallerySpace(view: ArtifactView) -> Element {
    rsx! {
        div { class: "gallery-space",
            for child in view.children.iter() {
                GalleryTile {
                    entry: child.clone(),
                    heat: child.heat,
                    onclick: move |_| navigate_to(child.id.clone()),
                }
            }
        }
    }
}

/// Request: central artifact with orbiting offers
#[component]
fn RequestSpace(view: ArtifactView) -> Element {
    rsx! {
        div { class: "request-space",
            div { class: "request-center",
                RequestContent { artifact: view.artifact.clone() }
            }
            div { class: "offer-orbit",
                for child in view.children.iter() {
                    OfferNode {
                        entry: child.clone(),
                        onclick: move |_| navigate_to(child.id.clone()),
                    }
                }
            }
        }
    }
}
```

---

## Navigation & Attention Tracking

The critical hook. Every navigation action writes an attention switch event to the player's iroh document.

```rust
/// Hook that manages navigation and attention tracking.
/// Every call to `navigate_to` logs an attention switch event.
fn use_navigation() -> UseNavigation {
    let mut nav = use_context::<Signal<NavigationState>>();
    let iroh = use_context::<Signal<IrohHandle>>();

    UseNavigation {
        // Enter a child artifact (zoom in)
        enter: move |artifact_id: ArtifactId| {
            let prev = nav.read().current_artifact.clone();

            // Log attention switch event to personal iroh doc
            let iroh = iroh.read();
            spawn(async move {
                iroh.log_attention_switch(&prev, &artifact_id).await;
            });

            // Update navigation state (triggers reactive re-render)
            nav.write().parent_stack.push(prev);
            nav.write().current_artifact = artifact_id;
        },

        // Return to parent (zoom out)
        back: move || {
            if let Some(parent) = nav.write().parent_stack.pop() {
                let prev = nav.read().current_artifact.clone();

                let iroh = iroh.read();
                spawn(async move {
                    iroh.log_attention_switch(&prev, &parent).await;
                });

                nav.write().current_artifact = parent;
            }
        },
    }
}
```

---

## iroh Integration Layer

A background service that runs the iroh node and bridges into Dioxus signals.

```rust
/// Spawned at app startup. Runs the iroh node and feeds state into signals.
async fn iroh_service(
    node: iroh::node::Node,
    artifact_view_tx: Signal<ArtifactView>,
    peer_events_tx: Signal<Vec<PeerPresence>>,
) {
    // Subscribe to document sync events
    let mut doc_events = node.docs().subscribe_all().await.unwrap();

    loop {
        tokio::select! {
            // A document we're syncing has new entries
            Some(event) = doc_events.next() => {
                match event {
                    DocEvent::InsertRemote { doc_id, entry, .. } => {
                        // A peer added something to a shared doc.
                        // If it's the currently-viewed artifact, update the view signal.
                        // If it's a peer's attention log, recompute heat values.
                        handle_doc_update(doc_id, entry, &artifact_view_tx).await;
                    }
                    DocEvent::NeighborUp { node_id, .. } => {
                        // A peer came online. Update presence.
                        update_presence(node_id, true, &peer_events_tx).await;
                    }
                    DocEvent::NeighborDown { node_id, .. } => {
                        update_presence(node_id, false, &peer_events_tx).await;
                    }
                    _ => {}
                }
            }
        }
    }
}
```

### Artifact Resolution

When the player navigates to an artifact, the system resolves what to show:

```rust
/// Resolves an artifact ID into a full ArtifactView.
/// - For Tree artifacts: reads the iroh doc's entries to get children.
/// - For Leaf artifacts: fetches the blob content on demand.
/// - Computes heat from synced peer attention logs.
async fn resolve_artifact(
    node: &iroh::node::Node,
    artifact_id: &ArtifactId,
    peer_registry: &PeerRegistry,
) -> ArtifactView {
    match artifact_id {
        ArtifactId::Doc(doc_id) => {
            let doc = node.docs().open(*doc_id).await.unwrap();

            // Read metadata
            let steward = doc.get_exact(AUTHOR, b"steward").await;
            let artifact_type = doc.get_exact(AUTHOR, b"type").await;

            // Read children references
            let children = doc.get_many(Query::key_prefix(b"ref/")).await
                .map(|entry| {
                    let child_id = ArtifactId::from_bytes(entry.content_bytes());
                    let heat = compute_heat(&child_id, peer_registry);
                    ChildEntry { id: child_id, heat, /* ... */ }
                })
                .collect::<Vec<_>>();

            // Check peer presence (who else has recent attention on this artifact?)
            let presence = get_peer_presence(artifact_id, peer_registry);

            ArtifactView { artifact: meta, children, peer_presence: presence, /* ... */ }
        }

        ArtifactId::Blob(hash) => {
            // Leaf artifact — fetch blob content lazily
            let content = node.blobs().read_to_bytes(*hash).await;
            ArtifactView::leaf(content)
        }
    }
}
```

---

## CSS Spatial System

The spatial browser's visual language is implemented in CSS. Dioxus 0.7 has built-in Tailwind support, but the spatial transitions and heat effects need custom CSS.

### Zoom Transitions

```css
/* Entering a child artifact — zoom in */
.artifact-space {
    transition: transform 0.3s ease-out, opacity 0.2s ease-out;
}

.artifact-space.entering {
    animation: zoom-in 0.3s ease-out forwards;
}

.artifact-space.exiting {
    animation: zoom-out 0.3s ease-in forwards;
}

@keyframes zoom-in {
    from {
        transform: scale(0.8);
        opacity: 0;
    }
    to {
        transform: scale(1);
        opacity: 1;
    }
}

@keyframes zoom-out {
    from {
        transform: scale(1);
        opacity: 1;
    }
    to {
        transform: scale(1.2);
        opacity: 0;
    }
}
```

### Heat (Attention Warmth)

```css
/* Heat is expressed as visual warmth, not a number */
.artifact-node {
    --heat: 0;
    transition: all 0.5s ease;
    filter: saturate(calc(0.3 + var(--heat) * 0.7))
            brightness(calc(0.85 + var(--heat) * 0.15));
    box-shadow: 0 0 calc(var(--heat) * 20px) calc(var(--heat) * 8px)
                rgba(255, 160, 60, calc(var(--heat) * 0.3));
}

/* Familiar artifacts (previously attended) have a subtle warmth */
.artifact-node.familiar {
    border-left: 2px solid rgba(255, 160, 60, 0.3);
}
```

### Peer Presence

```css
/* Ambient glow for artifacts where peers are present */
.peer-presence {
    position: absolute;
    border-radius: 50%;
    width: 8px;
    height: 8px;
    background: radial-gradient(circle, rgba(120, 200, 255, 0.8), transparent);
    animation: presence-pulse 3s ease-in-out infinite;
}

@keyframes presence-pulse {
    0%, 100% { opacity: 0.4; transform: scale(1); }
    50% { opacity: 0.8; transform: scale(1.3); }
}
```

### Applying Heat from Rust

```rust
#[component]
fn ArtifactNode(entry: ChildEntry, onclick: EventHandler<MouseEvent>) -> Element {
    let heat_style = format!("--heat: {:.2}", entry.heat);
    let familiar_class = if entry.familiar { "familiar" } else { "" };

    rsx! {
        div {
            class: "artifact-node {familiar_class}",
            style: "{heat_style}",
            onclick: move |e| onclick.call(e),

            // Type-specific preview
            match entry.artifact_type {
                ArtifactType::Leaf(LeafType::Image) => rsx! {
                    img { src: entry.preview.as_deref().unwrap_or("") }
                },
                ArtifactType::Leaf(LeafType::Message) => rsx! {
                    p { "{entry.preview.as_deref().unwrap_or(\"\")}" }
                },
                ArtifactType::Conversation => rsx! {
                    div { class: "conversation-preview",
                        span { class: "thread-indicator" }
                    }
                },
                _ => rsx! { div { class: "generic-preview" } },
            }
        }
    }
}
```

---

## Dioxus Router Integration

Dioxus has a built-in router. Each artifact path maps to a route, which means browser-style back/forward navigation works, and deep links into the artifact tree are possible.

```rust
use dioxus::prelude::*;

#[derive(Routable, Clone, PartialEq)]
enum Route {
    #[route("/")]
    Vault {},

    #[route("/artifact/:id")]
    Artifact { id: String },
}

fn App() -> Element {
    rsx! {
        Router::<Route> {}
    }
}

#[component]
fn Artifact(id: String) -> Element {
    let artifact_id = ArtifactId::from_string(&id);
    let view = use_artifact_view(artifact_id);

    rsx! {
        SpatialShell { view }
    }
}
```

Every route change triggers the attention switch hook. The router's navigation history *is* the player's attention trail.

---

## Hooks Summary

Custom hooks that encapsulate the core behaviors:

| Hook                    | Purpose                                                          |
|-------------------------|------------------------------------------------------------------|
| `use_navigation()`     | Navigate the artifact tree. Every call logs an attention switch.  |
| `use_artifact_view(id)` | Resolves an artifact ID into a reactive ArtifactView.            |
| `use_heat(id)`         | Computes perspectival heat for an artifact from peer attention logs. |
| `use_peer_presence(id)` | Returns which mutual peers are currently attending to this artifact. |
| `use_steward_controls(id)` | Audience management, stewardship transfer for artifacts you steward. |
| `use_iroh()`           | Access to the iroh node for direct operations (blob fetch, doc write). |
| `use_peer_registry()`  | The player's mutual peer list and their attention log doc IDs.    |

Each hook reads from Dioxus signals that the iroh service layer keeps updated.

---

## Project Structure

```
syncengine/
├── Cargo.toml
├── Dioxus.toml                  # Dioxus CLI config
├── assets/
│   └── styles/
│       ├── spatial.css          # Zoom transitions, heat, presence
│       └── tailwind.css         # Tailwind utilities
├── src/
│   ├── main.rs                  # App entry, launches iroh + dioxus
│   ├── app.rs                   # Root component, router, context providers
│   │
│   ├── components/
│   │   ├── mod.rs
│   │   ├── spatial_shell.rs     # The shell: breadcrumbs, presence, steward controls
│   │   ├── artifact_space.rs    # Polymorphic artifact renderer
│   │   ├── vault_space.rs       # Personal root space
│   │   ├── conversation_space.rs # Branching message tree
│   │   ├── gallery_space.rs     # Spatial image layout
│   │   ├── document_space.rs    # Vertical reading flow
│   │   ├── request_space.rs     # Request with orbiting offers
│   │   ├── exchange_space.rs    # Negotiation between two stewards
│   │   ├── leaf_view.rs         # Full content view for leaf artifacts
│   │   └── artifact_node.rs     # Reusable child node with heat visualization
│   │
│   ├── hooks/
│   │   ├── mod.rs
│   │   ├── use_navigation.rs    # Navigation + attention tracking
│   │   ├── use_artifact_view.rs # Artifact resolution
│   │   ├── use_heat.rs          # Perspectival heat computation
│   │   ├── use_peer_presence.rs # Live peer presence
│   │   └── use_steward.rs       # Stewardship controls
│   │
│   ├── iroh_layer/
│   │   ├── mod.rs
│   │   ├── node.rs              # iroh node setup and lifecycle
│   │   ├── service.rs           # Background service bridging iroh → signals
│   │   ├── artifacts.rs         # Artifact CRUD operations on iroh docs/blobs
│   │   ├── attention.rs         # Attention log read/write
│   │   ├── peering.rs           # Mutual peering protocol
│   │   └── integrity.rs         # Mutual witnessing / log consistency checks
│   │
│   └── types/
│       ├── mod.rs
│       ├── artifact.rs          # ArtifactId, ArtifactType, ArtifactMeta
│       ├── attention.rs         # AttentionSwitchEvent, Heat
│       ├── peer.rs              # PeerPresence, PeerRegistry
│       └── navigation.rs        # NavigationState, ZoomLevel
│
└── docs/
    ├── indras-network-spec.md   # Core data model
    └── ui-architecture.md       # UI architecture (the companion doc)
```

---

## Startup Sequence

```rust
fn main() {
    // 1. Initialize tokio runtime for iroh
    let rt = tokio::runtime::Runtime::new().unwrap();

    // 2. Boot iroh node (loads or creates identity, connects to relay)
    let node = rt.block_on(async {
        iroh::node::Node::persistent("/path/to/data")
            .await
            .unwrap()
            .spawn()
            .await
            .unwrap()
    });

    // 3. Launch Dioxus app, passing iroh node as context
    dioxus::LaunchBuilder::desktop()
        .with_context(node)
        .launch(App);
}

fn App() -> Element {
    let node = use_context::<iroh::node::Node>();

    // Create shared signals for the iroh service to write into
    let artifact_view = use_signal(|| ArtifactView::default());
    let peer_presence = use_signal(|| Vec::<PeerPresence>::new());

    // Spawn the iroh background service
    use_coroutine(move |_| {
        iroh_service(node, artifact_view, peer_presence)
    });

    // Provide signals as context for all child components
    use_context_provider(|| artifact_view);
    use_context_provider(|| peer_presence);

    rsx! {
        Router::<Route> {}
    }
}
```

---

## Development Workflow

```bash
# Install Dioxus CLI
cargo binstall dioxus-cli

# Create new project
dx new syncengine && cd syncengine

# Develop with hot-patching (subsecond rebuilds)
dx serve --hotpatch

# Build for desktop
dx bundle --platform desktop

# Build for web (same codebase)
dx bundle --platform web
```

Dioxus 0.7's hot-patching means you can edit RSX templates, styles, and even Rust logic and see changes without restarting the app or losing state. This is critical for iterating on the spatial UI — you need to feel the transitions and heat effects live.

---

## Key Advantages Over Tauri

| Concern                | Tauri                              | Dioxus                                    |
|------------------------|------------------------------------|-------------------------------------------|
| iroh integration       | Rust backend, JS frontend, IPC bridge | Direct Rust. No serialization boundary.   |
| Attention tracking     | IPC round-trip per event           | Same-process signal write. Instant.        |
| State management       | Duplicated: Rust state + JS state  | Single source: Dioxus signals from iroh.   |
| Type safety            | Lost at IPC boundary               | End-to-end Rust types.                     |
| Hot reload             | Vite for JS, restart for Rust      | Subsecond hot-patching for everything.     |
| Binary size            | ~8MB + WebView                     | ~12MB native or ~8MB + WebView             |
| Mobile                 | Limited                            | First-class iOS, experimental Android.     |
| Web target             | Not supported                      | Same codebase compiles to WASM.            |
