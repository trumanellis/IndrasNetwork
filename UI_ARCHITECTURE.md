# SyncEngine — UI Architecture

## Core Principle

The fractal artifact tree is not displayed *in* the UI. The fractal artifact tree *is* the UI. Each artifact is a place. The entire experience is wayfinding through a zoomable spatial structure.

---

## The Spatial Browser

SyncEngine's interface is a **spatial browser** — a zoomable canvas where navigation means entering and exiting artifacts. The player is always *inside* an artifact, looking at its children. Attending to a child means entering it. Going back means returning to the parent.

There is no global feed. No dashboard. No sidebar file tree. The player starts in their own root space and moves through the fractal by choosing where to place their attention.

**Navigation IS attention tracking.** Every zoom, every click into a child artifact, every return to a parent — these are attention switch events. The UI doesn't report events to a separate tracking layer. The act of moving through the interface *is* the event stream. There is no distinction between "using the app" and "generating attention data."

---

## Layers of the Interface

### 1. The Vault (Root Space)

The player's personal root. This is where you start. The Vault contains:

- **Artifacts you steward** — things you've created or received through exchange.
- **Artifacts shared with you** — things where you're in the audience, organized by how they arrived (which peer shared them, when).
- **Your peer connections** — visible as presences or directions you can move toward.

The Vault is itself a Tree Artifact. The player is its steward. Its spatial layout is personal — you arrange your own space however you want.

### 2. The Artifact Space (Zoom Level)

When you enter any artifact, you see a space shaped by its type:

- **Conversation** — a branching path. The trunk is the main thread. Forks are visible as side branches you can walk down. Messages appear as nodes along the path. The tree shape is the conversation shape.
- **Gallery** — a spatial field. Images are arranged by the tree artifact's layout coordinates. You can move among them, zoom into any one.
- **Document** — a vertical flow. Text, media, and embedded artifacts arranged in reading order. Entering an embedded artifact zooms you into it.
- **Request** — a central artifact with offers orbiting it. Tagged artifacts (offers of exchange) appear around the request, each one enterable.
- **Exchange** — a space between two artifacts and two stewards. The negotiation context. Contains a conversation tree and the two artifacts being discussed.

Each type is a different *spatial grammar* for the same underlying structure: a Tree Artifact and its children.

### 3. The Context Shell

Always visible around the current artifact space. Provides orientation without pulling you out of the space.

- **Breadcrumb trail** — where you are in the tree. Not a text path like `Vault > Gallery > Image` but a spatial sense: the parent space is visible behind/around the current view, receding with depth. You can see back through the spaces you came from.
- **Peer presence** — which of your mutual peers are currently attending to this same artifact. Shown as ambient indicators: warm glows, names at edges, subtle motion. Not avatars in a 3D space — just the feeling that you're not alone here.
- **Heat / Activity** — visual warmth on child artifacts that have recent attention from your peers. Brighter = more recent peer attention. This is the discovery mechanism: you see where your network's attention is flowing and can follow it.
- **Steward indicator** — who stewards this artifact. Subtle but always present. If it's you, you see your authority (can edit audience, transfer stewardship).

---

## Transitions

Since every navigation is an attention switch event, transitions between artifacts should feel **deliberate.** The player should feel that moving attention somewhere is a choice with weight.

**Entering a child** — zoom in. The child artifact expands to fill the space. The parent recedes but remains visible at the edges, providing context. The transition should feel like stepping into a room.

**Returning to parent** — zoom out. The current artifact shrinks back into its place among siblings. The parent space re-expands around you. The transition should feel like stepping back through a doorway.

**Lateral movement** — moving between siblings within the same parent space. Sliding, not zooming. Feels like turning to look at the next thing in the room.

**Following a peer** — if you see a peer's presence somewhere in the tree, you can follow them. This triggers a multi-level zoom that traces the path from your current location to theirs, making the journey through the tree visible.

**Transitions are not instant.** They are brief but perceptible. This creates a felt sense that attention has weight — that choosing to attend to something is a real action, not a free scroll.

---

## Attention as Visual Language

Attention is not a number displayed on a badge. It is the **visual warmth** of the interface.

- **Artifacts with high peer attention** glow warmer. Color temperature, luminosity, subtle animation — the space around an attended artifact feels alive.
- **Artifacts with low or no attention** are cooler, stiller, quieter. Not hidden — just calm.
- **Your own attention history** leaves traces. Artifacts you've spent time with feel familiar — a subtle visual difference from artifacts you haven't visited. Not a "read/unread" binary but a gradient of familiarity.
- **Recency matters.** An artifact that had intense peer attention yesterday but none today is cooling down. The warmth fades over time, creating a sense of the living now.

Because attention is perspectival, two players looking at the same artifact space see different warmth patterns. Your peer network's attention shapes your visual landscape. This is the subjective experience of perspectival value.

---

## Stewardship & Audience Controls

These are **spatial gestures**, not settings panels.

- **Sharing an artifact** (expanding the audience) — a gesture of opening. Drag the artifact toward a peer, or widen the artifact's boundary to include new peers. The audience list is visible as the boundary of the space.
- **Restricting an artifact** (narrowing the audience) — a gesture of closing. Contract the boundary. Peers outside the boundary can no longer sync or attend.
- **Transferring stewardship** — handing an artifact to another player. A deliberate gesture: push the artifact toward a peer and they accept it. The steward indicator shifts.
- **Tagging for exchange** — drag your artifact toward another artifact. This creates the exchange space between them. Both stewards can enter this space to negotiate.

These gestures should feel **physical.** You are handing things to people, opening doors, drawing boundaries. The social actions of the system have spatial metaphors.

---

## The Attention Log (Personal)

Each player has access to their own attention trail — a personal artifact (Tree type) that records the sequence of spaces they've moved through. This can be visualized as:

- **A path on a map** — your journey through the fractal tree rendered spatially. Where you've been, how long you lingered, which branches you explored.
- **A timeline** — your attention switches laid out chronologically. Useful for remembering "where was that thing I saw yesterday?"

This log is shared with mutual peers (it's the basis of the integrity system). But the visualization is personal — your map of your own journey.

---

## Information Density by Zoom Level

The fractal structure naturally manages information overload:

- **Zoomed far out** (Vault level) — you see high-level artifact groupings. Heat and activity are aggregated. Individual messages aren't visible; whole conversations glow or don't.
- **Zoomed to mid-level** (inside a Tree Artifact) — you see the children. Their individual heat, their types, their stewards. Enough detail to choose where to go next.
- **Zoomed in** (inside a Leaf or at the bottom of a branch) — you see the full content. The image, the message text, the request details. This is where you actually attend to content.

This mirrors how attention works: you notice activity at a distance, approach to investigate, and focus to engage. The UI zoom level is the attention granularity.

---

## No Notification System

There are no push notifications. Discovery is spatial.

When a peer shares something with you, it appears in your Vault space — a new artifact, glowing with the warmth of your peer's attention. You notice it when you next visit your Vault, the way you'd notice a gift left on your doorstep. 

Activity in conversations you're part of manifests as warmth on those conversation artifacts from wherever you are in the tree. You feel the pull of activity without being yanked out of your current context.

The only interruption is **presence** — the ambient awareness that peers are active somewhere in your shared spaces. This creates gentle social gravity, not urgent alerts.

---

## Responsive Form Factors

The spatial browser adapts to the device:

- **Desktop** — full spatial canvas. Mouse/trackpad for zooming, panning, entering. Generous space for the context shell and peer presence.
- **Tablet** — touch-native zooming. Pinch to zoom out, tap to enter. The spatial metaphor maps directly to touch gestures.
- **Mobile** — the spatial model compresses to a vertical stack. Entering an artifact is a forward navigation. The breadcrumb trail becomes a back gesture. Heat and presence are shown as compact indicators. The spatial *feeling* is preserved even in a linear layout.

---

## Implementation Notes

SyncEngine is a Dioxus desktop application. The entire stack — from iroh networking through UI rendering — is Rust. There is no IPC boundary between backend and frontend; iroh runs in-process with Dioxus. See `syncengine-dioxus-implementation.md` for the full implementation architecture.

**Rendering approach:** The spatial canvas is implemented using CSS transforms (scale/translate) for zoom levels, with transitions for enter/exit animations. This is sufficient for the core spatial experience. Migration to WGPU-based rendering is possible later via Dioxus's custom renderer support if the spatial grammar demands it.

**Data flow:**
1. iroh syncs documents and blobs in-process.
2. Custom Dioxus hooks subscribe to iroh document events and pipe changes into reactive signals.
3. Components read signals and render the spatial view.
4. Navigation actions write attention switch events to the player's iroh document and update UI state atomically — one action, one event.
5. Peer presence and attention heat are computed from synced attention logs and exposed as signals that drive visual warmth.

**Lazy loading:** Leaf artifact content (blob data) is only fetched when the player zooms into that artifact — the component mounting IS the fetch trigger. Tree structure (document keys) syncs eagerly. The UI's zoom level drives the replication priority.

---

## Design Principles

1. **The map is the territory.** The data structure and the interface are the same thing. Navigating the UI is navigating the artifact tree.
2. **Attention has weight.** Transitions are deliberate. Moving your focus somewhere is a real action that the system records and your peers can see.
3. **Warmth over numbers.** Attention value is expressed as visual temperature, not metrics. You feel activity; you don't count it.
4. **Presence over notification.** You sense your peers in the space. You are never interrupted, only drawn.
5. **Space over feed.** There is no timeline. There is only where you are, what's around you, and where your peers are.
