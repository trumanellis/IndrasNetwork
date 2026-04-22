# Progress — Project concept

## Phase 1 — Data model + manifest store ✅ DONE (2026-04-21)

**Outcome:** Reverse-direction snapshotting added; existing primitives reused for the rest.

**What we found in research (revised plan accordingly):**
- `PatchManifest` already exists at `crates/indras-sync-engine/src/braid/changeset.rs:36-68` — exact `path → hash` shape we needed. **No new manifest type added.**
- `Vault::apply_manifest` (vault/mod.rs:339-369) already materializes. We did **not** depend on it for Phase 1, since it requires full vault context (watcher, relay, local index) and would have made unit testing painful. Built a vault-free `materialize_to(manifest, dest, blob_store)` instead.
- `BlobStore::store/load` (`crates/indras-storage/src/blobs/store.rs`) handles dedup + verification.

**Files added:**
- `crates/indras-sync-engine/src/project/mod.rs` (24 lines) — module docs, re-exports.
- `crates/indras-sync-engine/src/project/snapshot.rs` (282 lines) — `snapshot_dir(dir, blob_store) -> PatchManifest`. Iterative DFS (no async-recursion crate). Skips top-level `DEFAULT_IGNORES` (`.git`, `.claude`, etc.). Path-sorted output for determinism. 5 round-trip unit tests.
- `crates/indras-sync-engine/src/project/materialize.rs` (50 lines) — `materialize_to(manifest, dest, blob_store)`. Free function, no Vault required.
- `crates/indras-sync-engine/src/lib.rs:83` — `pub mod project;`

**Tests:** `cargo test -p indras-sync-engine --lib project::` → 5 passed (empty_dir, dotfile_skip, manifest_is_sorted, round_trip_basic, incremental_hash_change).

**Build/clippy:** clean for new module; pre-existing warnings unchanged.

**Deviation from plan:** Did not introduce a new `ProjectManifest` type — reused `PatchManifest` because it already had the correct shape. Did not wrap `Vault::apply_manifest`; instead added a parallel free-function path that doesn't need a Vault instance (better testability, no functionality lost).

## Phase 2 — Wiring into Realm / vault ✅ DONE (2026-04-21)

**2A (RealmCategory + lifecycle):**
- `state.rs:87-106`: `RealmCategory::Project { parent: RealmId }` variant.
- `components/realm_column.rs`: 3 match statements extended to treat `Project { .. }` like `Group` (minimum compilable treatment — full layout decision deferred to Phase 4).
- `vault_manager.rs` additions: `project_heads: DashMap<[u8;32], ContentRef>`, `projects_by_parent: DashMap<[u8;32], Vec<[u8;32]>>`, `ProjectInfo` struct, `project_path / create_project / open_project / projects_of` (lines 248-362). `hex_bytes()` helper for full 64-char folder names (avoids `short_hex` prefix collisions).
- 4 round-trip tests added.
- Deviation: `home_vault.rs` and `vault_columns.rs` did **not** require updates — they construct variants rather than exhaustively match.

**2B (snapshot routing plumbing):**
- Renamed `land_agent_snapshot_on_first` → `land_agent_snapshot`, added `realm_id: Option<&[u8; 32]>` as first arg. `Some` routes by RealmId, `None` preserves first-vault fallback.
- Callers updated to pass `None`: `ipc.rs:331`, `sync_panel.rs:288`, `agent_lane.rs:618`, `braid_drawer.rs:234`. Zero remaining references to the old name.

**2C (/sync Project snapshot hook):**
- `VaultManager::snapshot_all_projects -> Result<Vec<[u8;32]>, String>`: iterates every registered Project, snapshots the folder, diffs ContentRef, updates `project_heads` only on change, returns changed IDs.
- Wired into `sync_panel.rs:277-283` pre-stage path in `commit_for_agent`. Errors logged as warnings (non-fatal — snapshot failure doesn't block commit).
- 2 new tests: `snapshot_all_projects_noop_when_unchanged`, `snapshot_all_projects_detects_file_add`.

**Verification (combined):** `cargo build -p synchronicity-engine` clean; `cargo test -p synchronicity-engine --lib vault_manager::` → 6/6.

## Phase 3 — Agent grouping + cooperative sandbox ✅ DONE (2026-04-21)

**Default-Project-per-Realm:**
- `VaultManager::default_project(parent) -> Result<[u8;32], String>` — returns first Project or auto-creates `"main"`. Lets existing realm-scoped `AgentRoster` keep working without a Phase 4 UI.

**Agent folder rewiring (`agent_lane.rs`):**
- `submit_create` now threads `vault_manager` into the async spawn. Non-private realms: resolve `default_project → project_path → {project_path}/agents/agent-{name}/`. Private vault (`[0u8;32]`) keeps the `{vault_path}/agent-{name}/` layout — explicit carve-out, commented.
- Agent roster filter now uses `filter_root`: `project_path` when a project exists; `vault_path` as bootstrap fallback before `default_project` first runs.
- Reuses `VaultManager::blob_store()` instead of opening fresh handles when available.
- `#[allow(clippy::too_many_arguments)]` on `submit_create` with justification.

**Cooperative PreToolUse sandbox:**
- `agent_hooks.rs::write_settings_template` gains `agent_sandbox_root: &Path`. Every hook command line now carries `--sandbox-root <path>`.
- `crates/indras-agent-hook/src/main.rs` rewritten:
  - New `--sandbox-root` CLI flag.
  - `path_in_sandbox(sandbox, target) -> bool` — canonicalizes both sides, walks to nearest existing ancestor for not-yet-created paths.
  - `extract_path(body)` — picks `tool_input.file_path / path / notebook_path / cwd` across common Claude Code tool schemas (Edit, Write, Read, NotebookEdit, Bash).
  - `enforce_sandbox(sandbox)`: reads stdin JSON on PreToolUse; exit 2 with stderr diagnostic on violation. Missing/unparseable stdin → exit 0 (fail-open, don't break existing hook flow).
  - Non-PreToolUse events unchanged.
- `tempfile = "3"` added as dev-dep.
- 5 new sandbox tests + 3 pre-existing payload tests all green.

**Identity layer:** `LogicalAgentId`, `Team.roster`, `WorkspaceHandle`, `runtime_bind` all **unchanged** — identity is namespace-free, only folder resolution and hook template needed changes.

**Verification:** `cargo test -p synchronicity-engine --lib vault_manager::` → 6/6; `cargo test -p indras-agent-hook` → 8/8.

## Phase 4 — UI ✅ CODE COMPLETE (2026-04-21)  ⚠️ visual check pending

**User chose layout (b): nested Projects under every Realm** (home, DMs, Groups, Worlds).

**Prep pass (layout-agnostic):**
- `AgentRoster` prop changed from `vault_realm: RealmId` to `project_id: RealmId + parent_realm: RealmId`. Mount sites resolve `default_project` outward instead of component-internal.
- `realm_column.rs`: all `RealmCategory::Project` match arms split out into dedicated arms (previously fell through Group's).
- `CreateProjectOverlay` scaffolded at `components/create_project.rs`, registered in `components/mod.rs`. Uses `use_callback` for Enter-to-submit + onclick reuse.

**Main pass (nested layout):**
- `state.rs`: `VaultSelection::selected_project: Option<RealmId>` + `AppState::show_create_project_for: Option<RealmId>`.
- `vault_manager.rs`: added `project_names: DashMap<[u8;32], String>` + `project_name(id)` getter. `create_project` populates it.
- `realm_column.rs` rewritten:
  - Top-level realm enumeration **filters out `RealmCategory::Project { .. }`** — Projects surface only as nested children.
  - Expanded realms render `projects_of(realm)` as indented `.project-row` children.
  - "+ New Project" inline row sets `show_create_project_for = Some(realm_id)` → opens overlay.
  - `AgentRoster` now mounts for the currently-selected Project (with `default_project` fallback) and only in the column owning the selection (prevents duplicate mounts).
- `home_vault.rs` mounts `CreateProjectOverlay` driven by `show_create_project_for`; on close with `Some(pid)`, expands parent + auto-selects new project.
- `styles.css`: `.project-row`, `.project-row--add`, `.project-row-bullet`, `.project-row-name` — indented one level deeper than files, same selection idiom, `↷` glyph for children, `+` for add row.

**Dead-code cleanup:**
- Removed `strip_agent_prefix` (was dead) and unused `mut state_w` in `agent_lane.rs`.

**Deviations:**
- Reused existing `.realm-chevron` / `.realm-entry` styles rather than adding a new disclosure affordance — minimally disruptive.
- Expansion state is per-realm only; `selected_project` is a separate field, so a realm can be expanded without any Project selected.
- Peer-synced Projects that lack a local name fall back to a 6-char hex label (so rows render instead of disappearing).

**Verification:** `cargo test -p synchronicity-engine --lib` → **37 passed**. `cargo build -p synchronicity-engine` clean. `cargo clippy` — no new warnings from the changed files.

**⚠️ Visual verification pending:** per CLAUDE.md ("For UI or frontend changes, start the dev server and use the feature in a browser before reporting the task as complete"), a live app launch should confirm: (1) chevron expansion UX, (2) `"main"` auto-creation on first expand, (3) `+ New Project` → overlay → indented row appears & auto-selects, (4) selecting a Project re-scopes AgentRoster, (5) agent folders land under `{project_path}/agents/`. Not done in-loop because the Dioxus desktop app grabs the user's window.
