# indras-ui

Shared Dioxus component library used by all Indras Network desktop apps. Provides the
7-skin design system, markdown rendering, identity display helpers, chat panel, artifact
gallery, navigation sidebar, slash menu, and detail panel. All CSS design tokens live
in `assets/shared.css` and are exported as `SHARED_CSS`.

## Module Map

```
src/
  lib.rs              — public re-exports and SHARED_CSS constant
  theme.rs            — Skin enum, ThemedRoot, SkinSwitcher, CURRENT_SKIN (7-skin system)
  markdown.rs         — render_markdown_to_html, is_markdown_file (pulldown-cmark)
  file_utils.rs       — load_image_as_data_url, load_text_file_content
  identity.rs         — member_name, reset_member_names, short_id,
                        format_duration_millis, member_color_class, member_color_var
  preview.rs          — PreviewFile, PreviewViewMode, PreviewContext, MarkdownPreviewOverlay
  contact_invite.rs   — ContactInviteOverlay
  artifact_display.rs — ArtifactDisplayInfo, ArtifactDisplayStatus, ArtifactGallery
  identity_row.rs     — IdentityRow
  peer_strip.rs       — PeerStrip, PeerDisplayInfo
  heat_display.rs     — HeatDot, HeatBar, heat_level
  navigation_sidebar.rs — NavigationSidebar, NavDestination, CreateAction, RecentItem
  slash_menu.rs       — SlashMenu, SlashAction
  detail_panel.rs     — DetailPanel, PropertyRow, AudienceMember, HeatEntry,
                        TrailEvent, ReferenceItem, SyncEntry
  chat.rs             — ChatPanel

assets/
  shared.css          — design tokens, theme definitions, base styles
```

## Key Types

- `Skin` / `ThemedRoot` — the 7-skin design system; wrap top-level app in `ThemedRoot`
  and switch themes via `SkinSwitcher` or `CURRENT_SKIN` signal
- `ChatPanel` — full chat UI component that talks to sync-engine; embeddable in any app
- `ArtifactGallery` — displays artifact list with `ArtifactDisplayInfo`/`ArtifactDisplayStatus`
- `NavigationSidebar` — left sidebar with `NavDestination` routing, `CreateAction` buttons,
  and `RecentItem` history
- `SlashMenu` — command palette overlay, emits `SlashAction` variants
- `DetailPanel` — right-side detail view with typed row types (`PropertyRow`, `HeatEntry`,
  `TrailEvent`, `ReferenceItem`, `SyncEntry`)
- `MarkdownPreviewOverlay` — full-screen preview of markdown or image files
- `ContactInviteOverlay` — modal for generating/sharing contact invite links
- `PeerStrip` / `PeerDisplayInfo` — horizontal list of connected peers with presence dots
- `HeatDot` / `HeatBar` — visual sync-heat indicators
- `IdentityRow` — single-line member identity display with color and short ID

## Key Patterns

- Every consumer app wraps its root component in `ThemedRoot` and injects `SHARED_CSS`
  (plus any app-specific CSS) into Dioxus desktop `with_custom_head`
- Identity helpers (`member_name`, `member_color_class`) require the consumer to populate
  a shared name cache via `reset_member_names` on startup
- `ChatPanel` is async-heavy; it spawns Tokio tasks to subscribe to sync-engine events

## Dependencies

| Crate | Role |
|---|---|
| `dioxus` (0.7, desktop) | UI framework |
| `pulldown-cmark` | Markdown → HTML |
| `base64` | Image data URLs |
| `indras-network` | Peer identity types, network handles |
| `indras-sync-engine` | Chat message subscription for ChatPanel |
| `indras-artifacts` | Artifact metadata for ArtifactGallery |
| `tokio` | Async task spawning inside components |
| `chrono` | Timestamp formatting |

## Testing

No automated tests (pure UI). Verify by running any consumer app (`indras-genesis`,
`indras-workspace`) and exercising the component visually. Skin switching is testable
by cycling through all 7 skins and checking CSS variable application.
