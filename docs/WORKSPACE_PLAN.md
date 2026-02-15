# Plan: Indras Workspace — Dioxus Desktop App with Real P2P

## Context

A frontend designer created `refs/indras_workspace.html` — a polished 3-panel workspace mockup showing how users interact with `indras-artifacts`. This plan implements it as a real Dioxus 0.7 desktop app connected to `indras-artifacts` for data and `indras-network` for P2P sync. Every navigation click generates a real `AttentionSwitchEvent`, heat is computed from real peer attention logs, and artifacts are stored/retrieved via the Vault API.

## Architecture

```
indras-workspace (new crate)          indras-ui (shared components)
├── main.rs         launch + config   ├── vault_sidebar.rs    (NEW)
├── state/                            ├── peer_strip.rs       (NEW)
│   ├── mod.rs                        ├── identity_row.rs     (NEW)
│   ├── workspace.rs   root state     ├── heat_display.rs     (NEW)
│   ├── navigation.rs  tree + crumbs  ├── detail_panel.rs     (NEW)
│   └── editor.rs      blocks + doc   ├── slash_menu.rs       (NEW)
├── components/                       └── (existing: theme, chat, gallery...)
│   ├── app.rs          3-panel grid
│   ├── topbar.rs       breadcrumbs
│   ├── document.rs     block editor
│   ├── blocks/         per-block-type renderers
│   │   ├── mod.rs
│   │   ├── text.rs
│   │   ├── heading.rs
│   │   ├── code.rs
│   │   ├── callout.rs
│   │   ├── todo.rs
│   │   ├── image.rs
│   │   └── divider.rs
│   ├── bottom_nav.rs   mobile nav
│   └── fab.rs          mobile FAB
├── bridge/
│   ├── mod.rs
│   ├── vault_bridge.rs  Vault ↔ Signal
│   └── network_bridge.rs  IndrasNetwork ↔ Signal
├── assets/
│   └── workspace.css   workspace-specific styles
└── Cargo.toml
```

## Critical Files to Modify/Reference

| File | Role |
|------|------|
| `/Cargo.toml` | Add `crates/indras-workspace` to workspace members |
| `crates/indras-ui/src/lib.rs` | Export 6 new shared components |
| `crates/indras-ui/assets/shared.css` | Add workspace theme variant + new component styles |
| `crates/indras-artifacts/src/vault.rs` | `Vault::in_memory()`, `navigate_to()`, `heat()`, `compose()` |
| `crates/indras-artifacts/src/attention.rs` | `compute_heat()`, `AttentionSwitchEvent` |
| `crates/indras-network/src/network.rs` | `IndrasNetwork::new()`, `.events()`, `.connect()` |
| `crates/indras-home-viewer/src/main.rs` | Reference pattern for Dioxus launch + async bridge |
| `refs/indras_workspace.html` | Visual specification (816 lines) |

## Mockup → Real API Mapping

### Sidebar

| Mockup Element | Component | Real API |
|----------------|-----------|----------|
| Identity avatar + name + ID | `IdentityRow` (indras-ui) | `network.identity_code()` → bech32m, `Member::name()` |
| Peer dots with online indicator | `PeerStrip` (indras-ui) | `vault.peers()` → `Vec<PeerEntry>`, `PeerEvent::Discovered/Lost` for online |
| Vault tree with folders/files | `VaultSidebar` (indras-ui) | `vault.root.references` → recursive traversal, `get_artifact()` per ref |
| Heat dots on tree items | `HeatDot` (indras-ui) | `vault.heat(&artifact_id, now)` → 0.0-1.0 → map to heat-0..heat-5 |
| Tree item click | Navigation handler | `vault.navigate_to(artifact_id, now)` → generates `AttentionSwitchEvent` |
| "New" button | Opens `SlashMenu` | — |

### Main Content

| Mockup Element | Component | Real API |
|----------------|-----------|----------|
| Breadcrumbs | `Topbar` (workspace) | Maintained in `NavigationState` from attention log history |
| Steward badge | `Topbar` | `artifact.steward == my_player_id` |
| Document title | `DocumentEditor` | `tree.metadata.get("title")` or first Message leaf |
| Doc meta (type, audience count, edited) | `DocumentEditor` | `artifact.artifact_type`, `artifact.audience.len()`, `artifact.created_at` |
| Content blocks | `blocks/*` renderers | Each `ArtifactRef` in `tree.references` → load child → render by `LeafType`/label |
| Slash menu | `SlashMenu` (indras-ui) | `vault.place_leaf()` / `vault.place_tree()` then `vault.compose()` |
| Todo checkboxes | `blocks/todo.rs` | Metadata toggle on the leaf's parent ref label |

### Detail Panel

| Mockup Element | Component | Real API |
|----------------|-----------|----------|
| Type, ID, Steward, Created, Refs | `DetailPanel` (indras-ui) | `artifact.artifact_type`, `.id`, `.steward`, `.created_at`, `.references.len()` |
| Audience list with roles | `DetailPanel` | `artifact.audience` → lookup each `PlayerId` in peer registry for name |
| Per-peer heat bars | `HeatBar` (indras-ui) | `vault.attention_value()` → per-peer dwell times from `peer_attention` |
| Combined heat bar | `HeatBar` | `vault.heat()` → combined f32 |
| Recent attention trail | `DetailPanel` | `attention_store.events_since(player, since)` → recent 5 events |
| References list | `DetailPanel` | `tree.references` → first few with type labels |

## State Design

```rust
// Shared via Signal<WorkspaceState> at root
pub struct WorkspaceState {
    pub nav: NavigationState,
    pub editor: EditorState,
    pub peers: PeerState,
    pub ui: UiState,
}

pub struct NavigationState {
    pub breadcrumbs: Vec<BreadcrumbEntry>,  // (ArtifactId, label)
    pub current_artifact: Option<ArtifactId>,
    pub expanded_nodes: HashSet<ArtifactId>,
    pub vault_tree: Vec<TreeNode>,          // Flattened tree for sidebar
}

pub struct EditorState {
    pub title: String,
    pub blocks: Vec<Block>,
    pub meta: DocumentMeta,  // type, audience_count, steward, created_at
}

pub struct PeerState {
    pub entries: Vec<PeerDisplayInfo>,  // id, name, online, color
    pub heat_values: HashMap<ArtifactId, Vec<PeerHeat>>,  // per-artifact, per-peer
}

pub struct UiState {
    pub sidebar_open: bool,       // mobile
    pub detail_open: bool,
    pub slash_menu_open: bool,
}
```

### Block Model

```rust
pub enum Block {
    Text { content: String, artifact_id: ArtifactId },
    Heading { level: u8, content: String, artifact_id: ArtifactId },
    Code { language: Option<String>, content: String, artifact_id: ArtifactId },
    Callout { content: String, artifact_id: ArtifactId },
    Todo { text: String, done: bool, artifact_id: ArtifactId },
    Image { caption: Option<String>, blob_id: ArtifactId, artifact_id: ArtifactId },
    Divider,
    Placeholder,
}
```

Blocks are derived from `tree.references` — each `ArtifactRef` label encodes the block type:
- `"text"` / `None` → Text block (load `LeafType::Message` payload as UTF-8)
- `"heading:2"` → Heading level 2
- `"code:rust"` → Code block with language
- `"callout"` → Callout
- `"todo"` / `"todo:done"` → Todo with checked state
- `"image"` → Image (load via `PayloadStore`)
- `"divider"` → Divider (no artifact, just a marker ref)

## Vault Bridge (async → signals)

```rust
// crates/indras-workspace/src/bridge/vault_bridge.rs
pub type InMemoryVault = Vault<InMemoryArtifactStore, InMemoryPayloadStore, InMemoryAttentionStore>;

// Shared across components via context
pub struct VaultHandle {
    vault: Arc<tokio::sync::Mutex<InMemoryVault>>,
    player_id: PlayerId,
}

// Initialization in RootApp:
// 1. Generate random PlayerId (or load from disk)
// 2. Vault::in_memory(player_id, now)
// 3. Seed initial vault structure (root tree + sample documents)
// 4. Wrap in Arc<Mutex<>> and provide via use_context_provider
```

## Network Bridge (P2P → signals)

```rust
// crates/indras-workspace/src/bridge/network_bridge.rs
// Following ChatPanel pattern from indras-ui

// In RootApp use_effect:
// 1. IndrasNetwork::new("~/.indras-workspace").await
// 2. network.start().await
// 3. Spawn event loop:
//    let mut events = network.events();
//    while let Some(global_event) = events.next().await {
//        match global_event.event {
//            InterfaceEvent::Custom { event_type: "attention_sync", payload, sender, .. } => {
//                // Deserialize Vec<AttentionSwitchEvent> from payload
//                // vault.ingest_peer_log(sender_player_id, events)
//                // Recompute heat for visible artifacts
//                // Update PeerState signal
//            }
//            InterfaceEvent::Presence { peer, status, .. } => {
//                // Update peer online/offline in PeerState
//            }
//            _ => {}
//        }
//    }
```

## Implementation Phases

### Phase 1: Skeleton (crate + static layout)

1. **Create `crates/indras-workspace/`** — Cargo.toml with deps: `dioxus 0.7 (desktop)`, `indras-artifacts`, `indras-network`, `indras-ui`, `tokio`, `serde`, `tracing`
2. **Add to workspace** — `Cargo.toml` members list
3. **`main.rs`** — Dioxus desktop launch with window config (1400x900), embedded CSS, Google Fonts
4. **`assets/workspace.css`** — Port the mockup's CSS (adapted to use `shared.css` design tokens where possible, keep workspace-specific styles like block editor, slash menu, mobile breakpoints)
5. **`components/app.rs`** — Static 3-panel grid layout matching mockup's `.app` grid

### Phase 2: Shared components in indras-ui

6. **`indras-ui/src/identity_row.rs`** — `IdentityRow { avatar_letter, name, short_id }` component
7. **`indras-ui/src/peer_strip.rs`** — `PeerStrip { peers: Vec<PeerDisplayInfo> }` with online dots
8. **`indras-ui/src/heat_display.rs`** — `HeatDot { level: u8 }` (0-5) and `HeatBar { label, value: f32, color }` components
9. **`indras-ui/src/vault_sidebar.rs`** — `VaultSidebar` with tree items, section labels, heat dots, expand/collapse, active selection
10. **`indras-ui/src/slash_menu.rs`** — `SlashMenu { visible, on_select: EventHandler<SlashAction> }` with all block types from mockup
11. **`indras-ui/src/detail_panel.rs`** — `DetailPanel` with properties, audience, heat bars, attention trail, references sections
12. **Update `indras-ui/src/lib.rs`** — Export all 6 new modules

### Phase 3: State + Vault integration

13. **`state/workspace.rs`** — `WorkspaceState` with all sub-states
14. **`state/navigation.rs`** — `NavigationState` with `rebuild_tree(&vault)`, `navigate(&vault, id, now)`, breadcrumb management
15. **`state/editor.rs`** — `EditorState` with `load_document(&vault, tree_id)` → parses refs into blocks
16. **`bridge/vault_bridge.rs`** — `VaultHandle` with `Arc<Mutex<InMemoryVault>>`, context provider, seed data

### Phase 4: Wire sidebar + navigation

17. **Wire `VaultSidebar`** — populate from `nav.vault_tree`, clicking calls `vault.navigate_to()` + rebuilds state
18. **Wire `Topbar`** — breadcrumbs from `nav.breadcrumbs`, steward badge from current artifact
19. **Wire `DetailPanel`** — properties, audience, heat from current artifact

### Phase 5: Document editor + blocks

20. **`components/document.rs`** — Loads current Tree(Document) children, renders block list
21. **`components/blocks/*.rs`** — 7 block renderers matching mockup's CSS classes (`.block`, `.block-code`, `.block-callout`, `.block-todo`, `.block-image`, etc.)
22. **Wire slash menu** — Creating artifacts via `vault.place_leaf()` + `vault.compose()`, adding blocks to current document

### Phase 6: P2P network integration

23. **`bridge/network_bridge.rs`** — `IndrasNetwork` init, event stream, attention sync protocol
24. **Wire peer strip** — Real peer discovery, online/offline from `PeerEvent`
25. **Wire heat** — `vault.ingest_peer_log()` from network events, recompute heat on attention sync
26. **Attention broadcast** — Periodically send own attention log to peers via `realm.send()` Custom events

### Phase 7: Mobile + polish

27. **`components/bottom_nav.rs`** — Mobile bottom navigation bar
28. **`components/fab.rs`** — Floating action button (opens slash menu)
29. **Responsive CSS** — Port all 4 breakpoints from mockup (1024px, 768px, 400px)
30. **Animations** — `fadeUp` stagger on content blocks, heat pulse, slide transitions

## Verification

1. `cargo build -p indras-workspace` compiles cleanly
2. `cargo build -p indras-ui` compiles with new components
3. `cargo test -p indras-artifacts` still passes (no modifications to artifacts crate)
4. `cargo run -p indras-workspace` launches desktop window with 3-panel layout
5. Clicking vault tree items generates `AttentionSwitchEvent` (verify via tracing logs)
6. Heat dots update when peer attention is ingested
7. Slash menu creates real artifacts visible in vault tree
8. Detail panel shows real properties from artifact store
9. Mobile breakpoints work when resizing window below 768px
