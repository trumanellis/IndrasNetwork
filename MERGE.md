# Merge Queue

## Pending

### Branch `worktree-homepage-feature`

**Connect profile visibility to the artifact grant system.**

Two commits on top of main:
1. `Add indras-homepage crate with axum HTTP profile server` — homepage server, templates, profile rendering
2. `Connect profile visibility to artifact grant system` — wire profile into ArtifactIndex grants

#### Files changed (15 files, +1015 -76 lines)

| File | Change |
|------|--------|
| `crates/indras-profile/` | **New crate**: Profile types, Visible<T>, ViewLevel, artifact ID helper |
| `crates/indras-profile/AGENTS.md` | Architectural docs |
| `crates/indras-homepage/src/grants.rs` | **New**: `resolve_view_level()` mapping grants to ViewLevel |
| `crates/indras-homepage/src/lib.rs` | Added grants module, grants/steward fields on HomepageServer |
| `crates/indras-homepage/src/server.rs` | AppState struct with profile + grants + steward |
| `crates/indras-homepage/src/templates.rs` | Rich profile page rendering with visibility controls |
| `crates/indras-homepage/src/profile.rs` | Profile re-exports |
| `crates/indras-homepage/Cargo.toml` | Added indras-artifacts dependency |
| `crates/indras-gift-cycle/src/app.rs` | Register profile artifact at boot, sync grants in poll loop |
| `crates/indras-gift-cycle/src/bridge.rs` | Added `grant_profile_access()` convenience method |
| `crates/indras-gift-cycle/Cargo.toml` | Added indras-artifacts dependency |
| `crates/indras-node/src/lib.rs` | Pass steward ID to HomepageServer::new() |
| `Cargo.toml` | Workspace member + dependency entries |
| `Cargo.lock` | Updated lockfile |

#### Merge steps

```bash
cd /Users/truman/Code/IndrasNetwork

# 1. Merge
git merge worktree-homepage-feature

# 2. Verify build
cargo build -p indras-profile -p indras-homepage -p indras-node -p indras-gift-cycle

# 3. Run tests
cargo test -p indras-profile -p indras-homepage

# 4. Clean up worktree
git worktree remove .claude/worktrees/homepage-feature

# 5. Delete the branch
git branch -d worktree-homepage-feature
```

#### Test summary

- `cargo build -p indras-profile -p indras-homepage -p indras-node -p indras-gift-cycle` — zero errors
- `cargo test -p indras-profile` — 8/8 pass
- `cargo test -p indras-homepage` — 14/14 pass (6 new grant resolution tests)
- Pre-existing: `indras-workspace` and `indras-simulation` have unrelated build errors (missing `QuestId` etc.)

---

## Completed

- **feature/artifact-browser** — merged into main (2026-02-22). 3-column artifact browser UI, artifact detail modal, navigation sidebar, audience popup. Worktree removed, branch deleted.
- **feature/telegram-chat** — merged into main (2026-02-22). New `indras-chat` crate with P2P Telegram-style chat, replaced workspace StoryView with embedded ChatLayout. Worktree removed, branch deleted. Conflict in app.rs resolved (import merge + `infer_artifact_type` re-applied).

Branch `feature/telegram-chat` (worktree: `IndrasNetwork-telegram-chat`)

## What Changed

New `indras-chat` crate: a standalone P2P Telegram-style chat app with Dioxus desktop UI. Also replaces the workspace's broken StoryView with an embedded `ChatLayout` from indras-chat. **23 files changed, +2184 -736 lines.**

### Commit 1: `af89c21` — Add indras-chat crate

New crate `crates/indras-chat/` — standalone Telegram-style chat app:

- **bridge.rs** — `NetworkHandle` wrapping `Arc<IndrasNetwork>`, identity creation/loading, platform-specific data dirs
- **state.rs** — `AppPhase`, `ChatContext` (Dioxus signals), `ConversationSummary`
- **components/app.rs** — `App` (root with first-run detection), `MainLayout` (sidebar + chat + contact add), `ChatLayout` (embeddable entry point accepting `NetworkArc`)
- **components/sidebar.rs** — Conversation list with last-message preview, add-contact button
- **components/chat_view.rs** — Message list with auto-scroll, send/reply functionality
- **components/message_bubble.rs** — Chat bubbles with sender avatar, timestamp, self/other styling
- **components/message_input.rs** — Compose bar with send button
- **components/contact_add.rs** — Add contact by identity URI
- **components/setup.rs** — First-run onboarding (name + optional PassStory)
- **style.css** — Full Telegram-style dark theme (792 lines)
- **main.rs** — Desktop app entry point with window geometry from env vars
- **se** — Launch script updates for multi-instance tiling

### Commit 2: `ee117ae` — Replace workspace StoryView with ChatLayout

- **indras-chat** made lib+bin crate (`[lib]` section, `lib.rs` re-exports)
- **Deleted** `crates/indras-workspace/src/components/story.rs` (347 lines) — `StoryView`, `StoryMessage`, `StoryArtifactRef`, `render_message` all removed
- **Simplified** `crates/indras-workspace/src/components/app.rs` (394 lines removed) — removed `chat_msg_to_story()`, `story_messages` signal, `network_for_chat` signal, CRDT chat doc subscription, all StoryMessage construction, entire ViewType::Story rendering block
- **ViewType::Story** now renders `indras_chat::components::app::ChatLayout` with the workspace's existing `Arc<IndrasNetwork>`
- **Chat CSS** injected via `indras_chat::CHAT_CSS` in workspace's `<style>` tags

## Merge Steps

```bash
cd /Users/truman/Code/IndrasNetwork

# 1. Merge
git merge feature/telegram-chat

# 2. Verify
cargo build -p indras-chat
cargo build -p indras-workspace

# 3. Clean up worktree
git worktree remove ../IndrasNetwork-telegram-chat

# 4. Delete the branch
git branch -d feature/telegram-chat
```

## If Not Fast-Forward

```bash
# Option A: Merge (preserves history)
git merge feature/telegram-chat

# Option B: Rebase branch first (linear history)
git checkout feature/telegram-chat
git rebase main
git checkout main
git merge feature/telegram-chat
```

Conflicts would likely be in:
- `crates/indras-workspace/src/components/app.rs` — if app component was restructured on main
- `crates/indras-workspace/src/components/mod.rs` — if modules were added/removed on main
- `Cargo.toml` / `Cargo.lock` — workspace member list

## Test Summary

- `cargo build -p indras-chat` — zero errors, zero warnings
- `cargo build -p indras-workspace` — zero errors, zero warnings
- StoryView fully removed, no dead code remaining

All pass as of commit `ee117ae`.
