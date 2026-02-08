# Visual Overhaul: indras-spatial Spatial Browser

## Context

The current implementation is a flat file browser — 5 grey tiles on a black background. The UI_ARCHITECTURE.md vision describes an immersive spatial experience where artifacts *glow* with attention warmth, navigation feels like stepping into rooms, and peer presence is felt as ambient indicators. This plan bridges that gap.

**Root cause of invisible heat:** `seed.rs` never calls `vault.ingest_peer_log()`, so `peer_attention` is empty and `compute_heat()` always returns 0.0. The CSS heat effects exist but multiply by zero.

**Root cause of flatness:** No glass morphism, no zoom transitions, no parent backdrop layer, no type-specific spatial layouts.

## Changes Overview

| File | What Changes |
|------|-------------|
| `seed.rs` | Inject peer attention events with large dwell times; return artifact IDs |
| `state.rs` | Add `TransitionKind`, `PeerPresence`, `ParentContext`; track transitions |
| `spatial.css` | Complete overhaul: glass morphism, heat glow, zoom keyframes, type layouts, peer presence, parent backdrop |
| `artifact_node.rs` | Add `data-type` attr, peer-dot indicators, richer structure |
| `artifact_space.rs` | Apply zoom transition CSS class from state |
| `spatial_shell.rs` | Add parent backdrop layer, peer presence bar |
| `story_space.rs` | Thread line connecting messages, node dots |
| `request_space.rs` | Centered description with offers arranged below with connectors |
| `exchange_space.rs` | Animated arrow, glass panels, richer status |
| `breadcrumb.rs` | Pill-shaped items with depth fading |
| `leaf_view.rs` | Glass morphism content card |

## Step 1: Fix Heat (Highest Impact)

### seed.rs — Inject Peer Attention

Change `populate()` to return artifact IDs and inject large-dwell peer events:

```rust
pub struct SeedIds {
    pub story_id: ArtifactId,
    pub request_id: ArtifactId,
    pub exchange_id: ArtifactId,
    pub dm_id: ArtifactId,
    pub inbox_id: ArtifactId,
}

pub fn populate(vault: &mut InMemoryVault, now: i64) -> SeedIds { ... }
```

After creating artifacts, inject peer logs with **large timestamp offsets** so dwell times reach the 60,000+ range needed for visible heat (SCALE=60,000 in `compute_heat`):

- Nova: 60,000s dwell on story + 30,000s on exchange -> story heat ~0.5, exchange ~0.33
- Sage: 40,000s on request + 30,000s on DM -> request heat ~0.4, DM ~0.33
- Orion: 50,000s on request + 30,000s on story -> request gets hotter (combined ~0.7)
- Lyra: 20,000s on inbox -> inbox heat ~0.25

Result: story=hot (multiple peers), request=very hot (3 peers), exchange=warm, DM=warm, inbox=cool.

### Key API Details (from code analysis)

- `AttentionSwitchEvent { player: PlayerId, from: Option<ArtifactId>, to: Option<ArtifactId>, timestamp: i64 }`
- `vault.ingest_peer_log(peer_id: PlayerId, events: Vec<AttentionSwitchEvent>) -> Result<()>`
- `compute_dwell_time` looks at `events.windows(2)` where `window[0].to == Some(artifact_id)`, dwell = `window[1].timestamp - window[0].timestamp`
- Timestamps are in seconds (from `chrono::Utc::now().timestamp()`)
- SCALE = 60,000 in `compute_heat` — so 60,000s dwell with recency=1.0 gives heat ~0.5
- HALF_LIFE_MS = 3,600,000 (but applied to age in same timestamp units, so effectively 3.6M seconds half-life — means recency factor stays ~1.0 for recent events)
- Peer must be in the artifact's `audience` to count

### spatial.css — Amplified Heat Glow

Replace the current subtle heat effect with dramatic glass morphism + multi-layer glow:

```css
.artifact-node {
    --heat: 0;
    background: rgba(30, 30, 30, calc(0.7 - var(--heat) * 0.2));
    backdrop-filter: blur(20px) saturate(calc(100% + var(--heat) * 80%));
    border: 1px solid rgba(255, 255, 255, calc(0.06 + var(--heat) * 0.15));
    box-shadow:
        0 0 calc(var(--heat) * 30px) rgba(255, 140, 50, calc(var(--heat) * 0.35)),
        0 0 calc(var(--heat) * 60px) rgba(255, 80, 30, calc(var(--heat) * 0.15)),
        inset 0 0 calc(var(--heat) * 20px) rgba(255, 160, 60, calc(var(--heat) * 0.1));
    transform: scale(calc(1 + var(--heat) * 0.05));
}
```

Plus a `::before` pseudo-element with `heatPulse` animation (breathing glow) that scales with `--heat`.

## Step 2: Glass Morphism Everything

Replace all flat `background: var(--bg-secondary)` panels with glass:

```css
background: rgba(30, 30, 30, 0.6);
backdrop-filter: blur(16px);
border: 1px solid rgba(255, 255, 255, 0.08);
```

Apply to: `.spatial-header`, `.story-message`, `.request-description`, `.offer-card`, `.exchange-side`, `.leaf-content`.

Add subtle radial gradients to `.spatial-shell` background for ambient depth.

## Step 3: Zoom Transitions

### state.rs — Transition Tracking

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum TransitionKind { None, ZoomIn, ZoomOut }
```

Add `transition: TransitionKind` and `transition_tick: u64` to `SpatialState`. Set on navigate, increment tick for CSS animation re-trigger.

### artifact_space.rs — Apply Transition Class

Use `state.transition` to set `class: "artifact-space zoom-in"` or `"artifact-space zoom-out"`, with `key: "{tick}"` to force re-mount and restart animation.

### spatial.css — Zoom Keyframes

```css
@keyframes zoomInEnter {
    from { opacity: 0; transform: scale(0.7); filter: blur(4px); }
    to   { opacity: 1; transform: scale(1);   filter: blur(0); }
}
@keyframes zoomOutEnter {
    from { opacity: 0; transform: scale(1.3); filter: blur(4px); }
    to   { opacity: 1; transform: scale(1);   filter: blur(0); }
}
.artifact-space.zoom-in  { animation: zoomInEnter 0.35s cubic-bezier(0.4, 0, 0.2, 1) both; }
.artifact-space.zoom-out { animation: zoomOutEnter 0.3s cubic-bezier(0.4, 0, 0.2, 1) both; }
```

## Step 4: Parent Backdrop Layer

### state.rs — Parent Context

```rust
pub struct ParentContext { pub label: String, pub child_labels: Vec<String>, pub depth: usize }
```

Captured on `navigate_into` before switching `current`. Cleared on navigate to root.

### spatial_shell.rs — Render Backdrop

Behind the main content: a fixed, blurred, scaled-up ghost of the parent space with very low opacity (0.05-0.08). Depth-indexed so deeper nesting = more faded.

## Step 5: Peer Presence Indicators

### state.rs — Simulated Peer Locations

```rust
pub struct PeerPresence { pub player: PlayerId, pub name: String, pub location: ArtifactId }
```

Seeded in `SpatialState::new()` using IDs from `seed::populate()`.

### artifact_node.rs — Peer Dots

Use `AttentionValue.unique_peers` to render colored pulsing dots on tiles (up to 5). Each dot gets a unique color from the member color palette (`--color-hope`, `--color-peace`, `--color-joy`, `--color-grace`).

### spatial_shell.rs — Presence Bar

In the header: small glowing pills showing peer names that are "nearby" (in current artifact or parent).

## Step 6: Type-Specific Visual Grammars

### Story — Thread Line

Add a vertical connecting line (via `::before` on `.story-flow`) with node dots (`::before` on each `.story-message`). Messages slide right on hover.

### Request — Centered + Radiating Offers

Description centered with special border glow. Offers arranged in a flex-wrap row below with connecting lines pointing up to the description.

### Exchange — Animated Arrow + Glass Panels

Replace `<>` with unicode arrow in a pulsing animation. Panels get type-specific border tint (`--color-love`).

### Node Icons — Type-Specific Colors

Add `data-type` attribute to `.artifact-node`. CSS targets: Story=cyan (`--color-peace`), Request=gold (`--color-joy`), Exchange=pink (`--color-love`), Inbox=purple (`--color-grace`).

## Step 7: Polish

- **Breadcrumb**: Pill-shaped items with depth-based opacity fade
- **Leaf view**: Large glass card with generous padding, centered
- **Grid spacing**: Increase gap to 24px to make room for glow effects
- **Back button**: Glass morphism treatment
- **Shell background**: Subtle radial gradients for ambient color

## Verification

1. `cargo check -p indras-spatial` — zero errors
2. `cargo run -p indras-spatial` — launch and verify:
   - Vault root shows tiles with **visible heat glow** (story and request brightest)
   - Colored peer dots pulse on hot tiles
   - Click story -> **zoom transition** (scale from 0.7, blur clears)
   - Parent backdrop faintly visible behind story messages
   - Thread line connects messages with node dots
   - Click back -> **zoom out** transition (scale from 1.3)
   - Breadcrumb shows depth-faded trail
   - Theme switcher works with glass morphism across all themes
   - Peer presence names glow in header bar
3. `cargo test -p indras-artifacts --test integration` — all 70 tests still pass

## File Paths (for reference)

All files under `crates/indras-spatial/`:
- `src/seed.rs`
- `src/state.rs`
- `src/lib.rs`
- `src/main.rs`
- `src/components/mod.rs`
- `src/components/artifact_node.rs`
- `src/components/artifact_space.rs`
- `src/components/spatial_shell.rs`
- `src/components/vault_space.rs`
- `src/components/story_space.rs`
- `src/components/request_space.rs`
- `src/components/exchange_space.rs`
- `src/components/breadcrumb.rs`
- `src/components/leaf_view.rs`
- `assets/spatial.css`
