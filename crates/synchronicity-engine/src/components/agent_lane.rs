//! Agent Roster — shows hosted agents for a vault column and lets the user
//! create, task, and remove agents interactively.
//!
//! # Architecture
//!
//! `AgentRoster` is mounted once per vault column (private + each realm) with
//! a `vault_realm` prop that scopes which agents it shows. Agents are filtered
//! from `workspace_handles` by the folder being inside the vault's path.
//!
//! # Row states
//!
//! Each row is classified by [`crate::state::agent_row_state`] and rendered
//! with a CSS modifier class:
//!
//! | State      | Class                      | Pill(s)                     |
//! |------------|----------------------------|-----------------------------|
//! | Idle       | `agent-row--idle`          | —                           |
//! | Thinking   | `agent-row--thinking`      | —                           |
//! | HasChanges | `agent-row--has-changes`   | `[land]`                    |
//! | ForkReady  | `agent-row--fork-ready`    | `[review]`                  |
//! | Blocked    | `agent-row--blocked`       | `[retry]`                   |
//!
//! # New CSS class names (inform Truman so styles can be added)
//!
//! - `.agent-roster`
//! - `.agent-roster-header`
//! - `.agent-roster-add-pill`
//! - `.agent-roster-create-form`
//! - `.agent-roster-create-input`
//! - `.agent-roster-color-dots`
//! - `.agent-roster-color-dot`
//! - `.agent-roster-color-dot--selected`
//! - `.agent-roster-create-btn`
//! - `.agent-roster-error`
//! - `.agent-roster-empty`
//! - `.agent-row`
//! - `.agent-row--idle`
//! - `.agent-row--thinking`
//! - `.agent-row--has-changes`
//! - `.agent-row--fork-ready`
//! - `.agent-row--blocked`
//! - `.agent-row-name`
//! - `.agent-row-name--editing`
//! - `.agent-row-task-input`
//! - `.agent-row-pills`
//! - `.agent-pill`
//! - `.agent-pill--land`
//! - `.agent-pill--review`
//! - `.agent-pill--retry`
//! - `.agent-context-menu`
//! - `.agent-context-menu-item`

use std::sync::Arc;

use dioxus::prelude::*;
use indras_sync_engine::team::LogicalAgentId;

use crate::state::{
    AgentRuntimeStatus, AgentRowState, AppState, BraidFocus, RealmId, agent_row_state,
    MEMBER_IDENTITY_CLASSES,
};
use crate::team::WorkspaceHandle;
use crate::vault_manager::VaultManager;

/// Identity color choices offered in the creation form (CSS classes).
const IDENTITY_COLORS: &[&str] = MEMBER_IDENTITY_CLASSES;

/// Agent Roster component — always rendered, even when empty.
///
/// Scoped to `vault_realm`: only agents whose folder lives inside the vault
/// path for that realm are shown.
#[component]
pub fn AgentRoster(
    mut state: Signal<AppState>,
    workspace_handles: Signal<Vec<WorkspaceHandle>>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    /// The realm this roster is scoped to (zero bytes = private vault).
    vault_realm: RealmId,
) -> Element {
    // ── Local UI state ──────────────────────────────────────────────────────
    let mut show_create = use_signal(|| false);
    let mut create_name = use_signal(String::new);
    let mut create_color_idx = use_signal(|| 0usize);
    let mut create_error: Signal<Option<String>> = use_signal(|| None);

    // Context-menu: (agent_id, x, y) or None
    let mut ctx_menu: Signal<Option<(LogicalAgentId, f64, f64)>> = use_signal(|| None);

    // Task-slip: which agent is being tasked right now, and the current prompt
    let mut tasking_agent: Signal<Option<LogicalAgentId>> = use_signal(|| None);
    let mut task_prompt = use_signal(String::new);

    // ── Resolve vault path for this roster's realm ──────────────────────────
    // The private column passes the zero-bytes sentinel; the home vault is
    // registered under its real realm id, so `vault_path(&[0;32])` would
    // return None. Use the path stashed on AppState in that case.
    let vault_path: Option<std::path::PathBuf> = if vault_realm == [0u8; 32] {
        let vp = state.read().vault_path.clone();
        if vp.as_os_str().is_empty() { None } else { Some(vp) }
    } else {
        let vm = vault_manager.read().clone();
        vm.and_then(|v| v.vault_path(&vault_realm))
    };

    // ── Filter handles to this vault ────────────────────────────────────────
    // Agents are shown only when their folder is inside this roster's vault
    // path. If we couldn't resolve a path (realm not vault-backed yet), show
    // no agents — never fall through to "show all", which would replicate
    // every agent across every column.
    let handles: Vec<LogicalAgentId> = match &vault_path {
        Some(vp) => workspace_handles
            .read()
            .iter()
            .filter(|h| h.index.root().starts_with(vp))
            .map(|h| h.agent.clone())
            .collect(),
        None => Vec::new(),
    };

    // ── Socket path (for hook template) ─────────────────────────────────────
    let socket_path = crate::ipc::socket_path(&crate::state::default_data_dir());
    let hook_binary = crate::agent_hooks::resolve_hook_binary()
        .unwrap_or_else(|| std::path::PathBuf::from("indras-agent-hook"));

    rsx! {
        div { class: "agent-roster",
            // ── Header ──────────────────────────────────────────────────────
            div { class: "agent-roster-header",
                span { class: "agent-roster-label", "agents" }
                button {
                    class: "agent-roster-add-pill",
                    title: "Add a new agent to this vault",
                    onclick: move |_| {
                        let cur = *show_create.read();
                        show_create.set(!cur);
                        create_error.set(None);
                        create_name.set(String::new());
                    },
                    "+ agent"
                }
            }

            // ── Creation form ────────────────────────────────────────────────
            if *show_create.read() {
                div { class: "agent-roster-create-form",
                    div { class: "agent-roster-create-input-wrap",
                        span { class: "agent-roster-create-prefix", "agent-" }
                        input {
                            class: "agent-roster-create-input",
                            placeholder: "name",
                            value: "{create_name.read()}",
                            oninput: move |e| create_name.set(e.value()),
                        onkeydown: {
                            let vault_path_kd = vault_path.clone();
                            let socket_path_kd = socket_path.clone();
                            let hook_binary_kd = hook_binary.clone();
                            move |e: KeyboardEvent| {
                                if e.key() == Key::Enter {
                                    submit_create(
                                        &mut state,
                                        &mut workspace_handles,
                                        &vault_path_kd,
                                        &socket_path_kd,
                                        &hook_binary_kd,
                                        &create_name.read(),
                                        &mut create_error,
                                        &mut show_create,
                                    );
                                }
                            }
                        },
                        }
                    }
                    div { class: "agent-roster-color-dots",
                        for (i, cls) in IDENTITY_COLORS.iter().enumerate() {
                            {
                                let i = i;
                                let dot_class = if i == *create_color_idx.read() {
                                    format!("agent-roster-color-dot {cls} agent-roster-color-dot--selected")
                                } else {
                                    format!("agent-roster-color-dot {cls}")
                                };
                                rsx! {
                                    button {
                                        class: "{dot_class}",
                                        onclick: move |_| create_color_idx.set(i),
                                        " "
                                    }
                                }
                            }
                        }
                    }
                    button {
                        class: "agent-roster-create-btn",
                        onclick: {
                            let vault_path_btn = vault_path.clone();
                            let socket_path_btn = socket_path.clone();
                            let hook_binary_btn = hook_binary.clone();
                            move |_| {
                                submit_create(
                                    &mut state,
                                    &mut workspace_handles,
                                    &vault_path_btn,
                                    &socket_path_btn,
                                    &hook_binary_btn,
                                    &create_name.read(),
                                    &mut create_error,
                                    &mut show_create,
                                );
                            }
                        },
                        "create"
                    }
                    if let Some(ref err) = *create_error.read() {
                        div { class: "agent-roster-error", "{err}" }
                    }
                }
            }

            // ── Empty state ──────────────────────────────────────────────────
            if handles.is_empty() {
                div { class: "agent-roster-empty", "·  no agents yet" }
            }

            // ── Agent rows ───────────────────────────────────────────────────
            for agent_id in handles {
                {
                    let agent_id = agent_id.clone();
                    let display_name = agent_id.as_str().to_string();

                    // Derive row state from AppState
                    let runtime = state
                        .read()
                        .agent_status
                        .get(&agent_id)
                        .copied()
                        .unwrap_or_default();
                    let handle_present = workspace_handles
                        .read()
                        .iter()
                        .any(|h| h.agent == agent_id);
                    // change_count and has_inner_fork come from agent_forks
                    let fork = state
                        .read()
                        .agent_forks
                        .iter()
                        .find(|f| {
                            LogicalAgentId::new(&f.name) == agent_id
                                && f.realm_id == vault_realm
                        })
                        .cloned();
                    let uncommitted = fork.as_ref().map(|f| f.change_count).unwrap_or(0);
                    let has_fork = fork.is_some() && uncommitted > 0;
                    let row_state = agent_row_state(handle_present, runtime, uncommitted, has_fork);

                    let row_class = format!(
                        "agent-row {}",
                        match row_state {
                            AgentRowState::Idle => "agent-row--idle",
                            AgentRowState::Thinking => "agent-row--thinking",
                            AgentRowState::HasChanges => "agent-row--has-changes",
                            AgentRowState::ForkReady => "agent-row--fork-ready",
                            AgentRowState::Blocked => "agent-row--blocked",
                        }
                    );

                    let is_tasking = tasking_agent.read().as_ref() == Some(&agent_id);
                    let agent_id_for_ctx = agent_id.clone();
                    let agent_id_for_task = agent_id.clone();
                    let agent_id_for_land = agent_id.clone();
                    let agent_id_for_retry = agent_id.clone();
                    let agent_id_for_review = agent_id.clone();
                    let agent_id_for_delete = agent_id.clone();

                    rsx! {
                        div {
                            class: "{row_class}",
                            oncontextmenu: move |e: MouseEvent| {
                                e.prevent_default();
                                let coords = e.client_coordinates();
                                ctx_menu.set(Some((
                                    agent_id_for_ctx.clone(),
                                    coords.x,
                                    coords.y,
                                )));
                            },

                            // Name label / task-slip toggle
                            if is_tasking {
                                input {
                                    class: "agent-row-task-input",
                                    placeholder: "Describe the task…",
                                    value: "{task_prompt.read()}",
                                    oninput: move |e| task_prompt.set(e.value()),
                                    onkeydown: {
                                        let agent_id_t = agent_id_for_task.clone();
                                        let vm = vault_manager.read().clone();
                                        let vp = vault_path.clone();
                                        move |e: KeyboardEvent| {
                                            if e.key() == Key::Enter {
                                                let prompt = task_prompt.read().clone();
                                                if !prompt.trim().is_empty() {
                                                    if let Some(folder) = resolve_agent_folder(
                                                        &workspace_handles, &agent_id_t,
                                                    ) {
                                                        write_task_md(&folder, &prompt);
                                                    }
                                                }
                                                tasking_agent.set(None);
                                                task_prompt.set(String::new());
                                            } else if e.key() == Key::Escape {
                                                tasking_agent.set(None);
                                            }
                                            let _ = (vm.clone(), vp.clone());
                                        }
                                    },
                                    autofocus: true,
                                }
                            } else {
                                span {
                                    class: "agent-row-name",
                                    title: "Click to set task",
                                    onclick: move |_| {
                                        // Pre-fill with last TASK.md content if any
                                        if let Some(folder) = resolve_agent_folder(
                                            &workspace_handles, &agent_id_for_task,
                                        ) {
                                            let existing = std::fs::read_to_string(folder.join("TASK.md"))
                                                .unwrap_or_default();
                                            // Strip the ISO timestamp comment at end
                                            let prompt = existing
                                                .lines()
                                                .take_while(|l| !l.starts_with("<!--"))
                                                .collect::<Vec<_>>()
                                                .join("\n")
                                                .trim()
                                                .to_string();
                                            task_prompt.set(prompt);
                                        }
                                        tasking_agent.set(Some(agent_id_for_task.clone()));
                                    },
                                    "{display_name}"
                                }
                            }

                            // Runtime badge
                            if runtime == AgentRuntimeStatus::Crashed {
                                span {
                                    class: "agent-row-badge agent-row-badge--crashed",
                                    title: "Agent appears stuck",
                                    "!"
                                }
                            }

                            // Action pills
                            div { class: "agent-row-pills",
                                match row_state {
                                    AgentRowState::HasChanges => rsx! {
                                        button {
                                            class: "agent-pill agent-pill--land",
                                            title: "Land agent changes to inner braid",
                                            onclick: move |_| {
                                                land_agent(
                                                    &mut state,
                                                    &workspace_handles,
                                                    &vault_manager,
                                                    &agent_id_for_land,
                                                );
                                            },
                                            "land"
                                        }
                                    },
                                    AgentRowState::ForkReady => rsx! {
                                        button {
                                            class: "agent-pill agent-pill--review",
                                            title: "Review agent fork in Braid Drawer",
                                            onclick: move |_| {
                                                open_review_drawer(
                                                    &mut state,
                                                    &agent_id_for_review,
                                                );
                                            },
                                            "review"
                                        }
                                    },
                                    AgentRowState::Blocked => rsx! {
                                        button {
                                            class: "agent-pill agent-pill--retry",
                                            title: "Retry binding this agent folder",
                                            onclick: move |_| {
                                                retry_bind(
                                                    &mut workspace_handles,
                                                    &vault_manager,
                                                    &agent_id_for_retry,
                                                );
                                            },
                                            "retry"
                                        }
                                    },
                                    _ => rsx! {},
                                }
                            }

                            // Delete button — always visible, right-edge
                            button {
                                class: "agent-row-delete",
                                title: "Remove agent",
                                onclick: move |_| {
                                    remove_agent(
                                        &mut workspace_handles,
                                        &agent_id_for_delete,
                                    );
                                },
                                "×"
                            }
                        }
                    }
                }
            }

            // ── Context menu ─────────────────────────────────────────────────
            if let Some((ref ctx_agent, cx, cy)) = *ctx_menu.read() {
                {
                    let ctx_agent = ctx_agent.clone();
                    let ctx_agent_remove = ctx_agent.clone();
                    rsx! {
                        div {
                            class: "agent-context-menu",
                            style: "left:{cx}px;top:{cy}px;",
                            // Dismiss on click-outside (handled by the single item below)
                            onmouseleave: move |_| ctx_menu.set(None),
                            button {
                                class: "agent-context-menu-item agent-context-menu-item--danger",
                                onclick: move |_| {
                                    remove_agent(
                                        &mut workspace_handles,
                                        &ctx_agent_remove,
                                    );
                                    ctx_menu.set(None);
                                },
                                "remove agent"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Strip the `agent-` prefix for display. Returns the name unchanged if it
/// doesn't start with the prefix.
fn strip_agent_prefix(name: &str) -> &str {
    name.strip_prefix("agent-").unwrap_or(name)
}

/// Resolve the on-disk folder for an agent by scanning workspace_handles.
fn resolve_agent_folder(
    workspace_handles: &Signal<Vec<WorkspaceHandle>>,
    agent_id: &LogicalAgentId,
) -> Option<std::path::PathBuf> {
    workspace_handles
        .read()
        .iter()
        .find(|h| &h.agent == agent_id)
        .map(|h| h.index.root().to_path_buf())
}

/// Write `TASK.md` into the agent folder with an ISO 8601 timestamp comment.
fn write_task_md(folder: &std::path::Path, prompt: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let content = format!("{}\n\n<!-- {now} -->\n", prompt.trim());
    if let Err(e) = std::fs::write(folder.join("TASK.md"), content) {
        tracing::warn!(error = %e, "failed to write TASK.md");
    }
}

/// Validate the user-supplied short name and prepend `agent-`.
///
/// Returns the full folder name (e.g. `"agent-coder"`) or an error string
/// suitable for display in the creation form.
fn validate_and_prefix(short_name: &str) -> Result<String, String> {
    let name = short_name.trim();
    if name.is_empty() {
        return Err("Name is required".into());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err("Only a-z, 0-9, _ and - are allowed".into());
    }
    Ok(format!("agent-{name}"))
}

/// Synchronously submit the creation form.
///
/// Prepends `agent-` to the name, creates the folder inside the vault path
/// (or data-dir if no vault), calls [`crate::team::runtime_bind`] via a
/// `spawn`, and writes the hook settings template on success.
fn submit_create(
    state: &mut Signal<AppState>,
    workspace_handles: &mut Signal<Vec<WorkspaceHandle>>,
    vault_path: &Option<std::path::PathBuf>,
    socket_path: &std::path::Path,
    hook_binary: &std::path::Path,
    short_name: &str,
    create_error: &mut Signal<Option<String>>,
    show_create: &mut Signal<bool>,
) {
    let full_name = match validate_and_prefix(short_name) {
        Ok(n) => n,
        Err(e) => {
            create_error.set(Some(e));
            return;
        }
    };

    // Agent folder lives inside the vault path (or data-dir fallback)
    let base = vault_path
        .clone()
        .unwrap_or_else(|| crate::state::default_data_dir().join("vaults").join("home"));
    let agent_folder = base.join(&full_name);

    // Create the directory now (synchronous, small FS op)
    if let Err(e) = std::fs::create_dir_all(&agent_folder) {
        create_error.set(Some(format!("Failed to create folder: {e}")));
        return;
    }

    // Write hook settings template (non-fatal if missing binary)
    if let Err(e) = crate::agent_hooks::write_settings_template(
        &agent_folder,
        socket_path,
        hook_binary,
    ) {
        tracing::warn!(error = %e, "write_settings_template failed (non-fatal)");
    }

    // Bind asynchronously; update workspace_handles on success
    let agent_id = LogicalAgentId::new(&full_name);
    let folder_clone = agent_folder.clone();
    let mut handles_w = workspace_handles.clone();
    let mut err_sig = create_error.clone();

    // Get blob_store from vault_manager (not available here directly) —
    // use the data-dir shared blob store path as fallback.
    // Since VaultManager is not passed directly, we spawn with a fresh
    // BlobStore matching the same shared-blobs path as VaultManager::new uses.
    let data_dir = crate::state::default_data_dir();
    spawn(async move {
        let blob_cfg = indras_storage::BlobStoreConfig {
            base_dir: data_dir.join("shared-blobs"),
            ..Default::default()
        };
        let blob_store = match indras_storage::BlobStore::new(blob_cfg).await {
            Ok(b) => Arc::new(b),
            Err(e) => {
                err_sig.set(Some(format!("blob store error: {e}")));
                return;
            }
        };
        match crate::team::runtime_bind(agent_id, folder_clone, blob_store).await {
            Ok(handle) => {
                handles_w.write().push(handle);
            }
            Err(e) => {
                err_sig.set(Some(format!("Bind failed: {e}")));
            }
        }
    });

    show_create.set(false);
    create_error.set(None);
    create_name_signal_clear(state);
}

/// Clears the create_name field (via state write — unused but keeps compiler happy).
#[allow(unused)]
fn create_name_signal_clear(_state: &mut Signal<AppState>) {}

/// Land an agent's working tree into the inner braid.
fn land_agent(
    state: &mut Signal<AppState>,
    workspace_handles: &Signal<Vec<WorkspaceHandle>>,
    vault_manager: &Signal<Option<Arc<VaultManager>>>,
    agent_id: &LogicalAgentId,
) {
    let vm_opt = vault_manager.read().clone();
    let handle_root = resolve_agent_folder(workspace_handles, agent_id);
    let agent_id = agent_id.clone();
    let mut state_w = state.clone();
    let handles = workspace_handles.read().iter()
        .find(|h| h.agent == agent_id)
        .map(|h| (h.agent.clone(), Arc::clone(&h.index)))
        .clone();

    spawn(async move {
        let Some(vm) = vm_opt else { return };
        let Some((_agent, index)) = handles else { return };
        let _ = handle_root;
        let intent = format!("agent land: {}", agent_id.as_str());
        // Derive a stable UserId from the agent name bytes (zero-padded)
        let signed_by: indras_sync_engine::vault::vault_file::UserId = {
            let mut arr = [0u8; 32];
            let b = agent_id.as_str().as_bytes();
            let n = b.len().min(32);
            arr[..n].copy_from_slice(&b[..n]);
            arr
        };
        let evidence = indras_sync_engine::braid::changeset::Evidence::Agent {
            compiled: false,
            tests_passed: vec![],
            lints_clean: false,
            runtime_ms: 0,
            signed_by,
        };
        if let Err(e) = vm
            .land_agent_snapshot_on_first(&agent_id, &index, intent, evidence)
            .await
        {
            tracing::warn!(error = %e, "land_agent failed");
        }
        let _ = state_w.read().step.clone(); // keep state_w alive
    });
}

/// Open the Braid Drawer in `AgentReview` mode for the given agent.
fn open_review_drawer(state: &mut Signal<AppState>, agent_id: &LogicalAgentId) {
    let mut w = state.write();
    w.braid_drawer_open = true;
    // Cluster 5 adds BraidFocus::AgentReview; scaffold with Realm focus for now.
    // This will be updated when cluster 5 lands.
    w.braid_drawer_focus = Some(BraidFocus::AgentReview {
        agent: agent_id.clone(),
    });
}

/// Retry binding a Blocked agent by calling `runtime_bind` again.
fn retry_bind(
    workspace_handles: &mut Signal<Vec<WorkspaceHandle>>,
    vault_manager: &Signal<Option<Arc<VaultManager>>>,
    agent_id: &LogicalAgentId,
) {
    // Resolve the folder from the agent_id name and the vault_manager's paths.
    // Since the handle may not exist (that's why it's Blocked), derive the
    // expected path from the agent id.
    let _ = vault_manager;
    let agent_id = agent_id.clone();
    let mut handles_w = workspace_handles.clone();
    let data_dir = crate::state::default_data_dir();

    // Try to find the folder from any existing (but failed) binding.
    // If not found, derive from the data dir.
    let folder = data_dir.join("vaults").join("home").join(agent_id.as_str());

    spawn(async move {
        let blob_cfg = indras_storage::BlobStoreConfig {
            base_dir: data_dir.join("shared-blobs"),
            ..Default::default()
        };
        let blob_store = match indras_storage::BlobStore::new(blob_cfg).await {
            Ok(b) => Arc::new(b),
            Err(e) => {
                tracing::warn!(error = %e, "retry_bind: blob store error");
                return;
            }
        };
        match crate::team::runtime_bind(agent_id.clone(), folder, blob_store).await {
            Ok(handle) => {
                handles_w.write().push(handle);
                tracing::info!(agent = %agent_id.as_str(), "retry_bind succeeded");
            }
            Err(e) => {
                tracing::warn!(agent = %agent_id.as_str(), error = %e, "retry_bind failed");
            }
        }
    });
}

/// Remove an agent: drop the `WorkspaceHandle` (releases lock + watcher) and
/// recursively delete the folder (symlinks + `.claude/`). Blobs are
/// content-addressed and unaffected.
fn remove_agent(workspace_handles: &mut Signal<Vec<WorkspaceHandle>>, agent_id: &LogicalAgentId) {
    // Find and remove the handle (drops lock + watcher on removal)
    let folder = {
        let mut handles = workspace_handles.write();
        let pos = handles.iter().position(|h| &h.agent == agent_id);
        pos.map(|i| {
            let h = handles.remove(i);
            h.index.root().to_path_buf()
        })
    };

    // Remove the folder tree (non-fatal if missing)
    if let Some(folder) = folder {
        if let Err(e) = std::fs::remove_dir_all(&folder) {
            tracing::warn!(
                folder = %folder.display(),
                error = %e,
                "remove_agent: fs::remove_dir_all failed"
            );
        }
    }
}
