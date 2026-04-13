//! Relay-node settings overlay — frictionless inline editing.
//!
//! Reuses `.file-modal-overlay` / `.file-modal` chrome from `file_modal`.
//! All edits autosave on blur; no save / apply / confirm buttons.
//!
//! The overlay has two sections:
//! 1. **P2P relay** — server list, LAN-only toggle, preset selector (local config).
//! 2. **Admin config** — display_name, quota, storage, and per-tier limits fetched
//!    from `GET /config` and persisted via `PUT /config` on blur.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::admin_client::AdminClient;
use crate::config::PRESET_NAMES;
use crate::state::AppState;
use indras_network::IndrasNetwork;
use indras_relay::{QuotaConfigPatch, RelayConfigPatch, RelayConfigView, StorageConfigPatch, TierConfigPatch};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Truncate a hex string to `head…tail` form for display.
fn truncate_hex(s: &str) -> String {
    if s.len() <= 20 {
        s.to_string()
    } else {
        format!("{}…{}", &s[..10], &s[s.len() - 6..])
    }
}

/// Persist the cached `relay_config` to disk.
fn persist(state: &Signal<AppState>) {
    let cfg = state.read().relay_config.clone();
    let _ = cfg.save();
}

/// Format bytes as a human-readable hint string ("100 MB", "1 GB").
fn format_bytes_hint(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.0} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// Parse a human-friendly byte string ("1 GB", "500mb", "52428800") into bytes.
/// Returns `None` if the input is empty or unparseable.
fn parse_bytes(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }
    // Try plain integer first
    if let Ok(n) = s.parse::<u64>() {
        return Some(n);
    }
    // Try "NNN unit" or "NNNunit"
    let (num_part, unit) = if let Some(pos) = s.find(|c: char| c.is_alphabetic()) {
        (&s[..pos], s[pos..].trim())
    } else {
        return None;
    };
    let n: f64 = num_part.trim().parse().ok()?;
    let multiplier = match unit {
        "kb" | "k" => 1024_f64,
        "mb" | "m" => 1024.0 * 1024.0,
        "gb" | "g" => 1024.0 * 1024.0 * 1024.0,
        "tb" | "t" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        "b" => 1.0,
        _ => return None,
    };
    Some((n * multiplier) as u64)
}

// ── Admin config panel ────────────────────────────────────────────────────────

/// Save state for a single field — shows "saved" briefly or an inline error.
#[derive(Clone, PartialEq)]
enum SaveState {
    Idle,
    Saving,
    Saved,
    Error(String),
}

/// Inner component that renders the admin config form once the config is loaded.
/// Receives a clone of `RelayConfigView` and a callback to trigger a PUT.
#[component]
fn AdminConfigPanel(
    cfg: RelayConfigView,
    on_patch: EventHandler<RelayConfigPatch>,
) -> Element {
    // Local draft signals for each field (string-typed for input binding)
    let mut display_name = use_signal(|| cfg.display_name.clone());

    let mut quota_bytes_per_peer = use_signal(|| format_bytes_hint(cfg.quota.default_max_bytes_per_peer));
    let mut quota_ifaces = use_signal(|| cfg.quota.default_max_interfaces_per_peer.to_string());
    let mut quota_global = use_signal(|| format_bytes_hint(cfg.quota.global_max_bytes));

    let mut storage_default_ttl = use_signal(|| cfg.storage.default_event_ttl_days.to_string());
    let mut storage_max_ttl = use_signal(|| cfg.storage.max_event_ttl_days.to_string());
    let mut storage_cleanup = use_signal(|| cfg.storage.cleanup_interval_secs.to_string());

    let mut self_bytes = use_signal(|| format_bytes_hint(cfg.tiers.self_max_bytes));
    let mut self_ttl = use_signal(|| cfg.tiers.self_ttl_days.to_string());
    let mut self_ifaces = use_signal(|| cfg.tiers.self_max_interfaces.to_string());

    let mut conn_bytes = use_signal(|| format_bytes_hint(cfg.tiers.connections_max_bytes));
    let mut conn_ttl = use_signal(|| cfg.tiers.connections_ttl_days.to_string());
    let mut conn_ifaces = use_signal(|| cfg.tiers.connections_max_interfaces.to_string());

    let mut pub_bytes = use_signal(|| format_bytes_hint(cfg.tiers.public_max_bytes));
    let mut pub_ttl = use_signal(|| cfg.tiers.public_ttl_days.to_string());
    let mut pub_ifaces = use_signal(|| cfg.tiers.public_max_interfaces.to_string());

    // Per-field save state signals
    let ss_name = use_signal(|| SaveState::Idle);
    let ss_quota_bytes = use_signal(|| SaveState::Idle);
    let ss_quota_ifaces = use_signal(|| SaveState::Idle);
    let ss_quota_global = use_signal(|| SaveState::Idle);
    let ss_storage_default = use_signal(|| SaveState::Idle);
    let ss_storage_max = use_signal(|| SaveState::Idle);
    let ss_storage_cleanup = use_signal(|| SaveState::Idle);
    let ss_self_bytes = use_signal(|| SaveState::Idle);
    let ss_self_ttl = use_signal(|| SaveState::Idle);
    let ss_self_ifaces = use_signal(|| SaveState::Idle);
    let ss_conn_bytes = use_signal(|| SaveState::Idle);
    let ss_conn_ttl = use_signal(|| SaveState::Idle);
    let ss_conn_ifaces = use_signal(|| SaveState::Idle);
    let ss_pub_bytes = use_signal(|| SaveState::Idle);
    let ss_pub_ttl = use_signal(|| SaveState::Idle);
    let ss_pub_ifaces = use_signal(|| SaveState::Idle);

    // Helper: render a small save-state badge
    let save_badge = |ss: &Signal<SaveState>| -> Element {
        match &*ss.read() {
            SaveState::Idle | SaveState::Saving => rsx! {},
            SaveState::Saved => rsx! { span { class: "admin-save-ok", "saved" } },
            SaveState::Error(e) => rsx! { span { class: "admin-save-err", "{e}" } },
        }
    };

    rsx! {
        // ── General ──────────────────────────────────────────────────────────
        div { class: "relay-panel",
            div { class: "relay-panel-header", "GENERAL" }
            div { class: "relay-panel-body",
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "DISPLAY NAME" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-text-input",
                            r#type: "text",
                            value: "{display_name}",
                            oninput: move |e| display_name.set(e.value()),
                            onblur: {
                                let mut ss = ss_name.clone();
                                move |_| {
                                    let val = display_name.read().trim().to_string();
                                    if val.is_empty() {
                                        ss.set(SaveState::Error("name cannot be empty".to_string()));
                                        return;
                                    }
                                    let patch = RelayConfigPatch {
                                        display_name: Some(val),
                                        ..Default::default()
                                    };
                                    ss.set(SaveState::Saving);
                                    on_patch.call(patch);
                                    let mut ss2 = ss.clone();
                                    spawn(async move {
                                        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                        if *ss2.read() == SaveState::Saved {
                                            ss2.set(SaveState::Idle);
                                        }
                                    });
                                    ss.set(SaveState::Saved);
                                }
                            },
                        }
                        {save_badge(&ss_name)}
                    }
                }
            }
        }

        // ── Quotas ───────────────────────────────────────────────────────────
        div { class: "relay-panel",
            div { class: "relay-panel-header", "QUOTAS" }
            div { class: "relay-panel-body",

                // bytes per peer
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "PER-PEER MAX" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text",
                            placeholder: "e.g. 100 MB",
                            value: "{quota_bytes_per_peer}",
                            oninput: move |e| quota_bytes_per_peer.set(e.value()),
                            onblur: {
                                let mut ss = ss_quota_bytes.clone();
                                move |_| {
                                    let raw = quota_bytes_per_peer.read().clone();
                                    match parse_bytes(&raw) {
                                        None => ss.set(SaveState::Error("invalid size".to_string())),
                                        Some(v) => {
                                            quota_bytes_per_peer.set(format_bytes_hint(v));
                                            let patch = RelayConfigPatch {
                                                quota: Some(QuotaConfigPatch {
                                                    default_max_bytes_per_peer: Some(v),
                                                    ..Default::default()
                                                }),
                                                ..Default::default()
                                            };
                                            on_patch.call(patch);
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved {
                                                    ss2.set(SaveState::Idle);
                                                }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_quota_bytes)}
                    }
                }

                // global max
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "GLOBAL MAX" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text",
                            placeholder: "e.g. 10 GB",
                            value: "{quota_global}",
                            oninput: move |e| quota_global.set(e.value()),
                            onblur: {
                                let mut ss = ss_quota_global.clone();
                                move |_| {
                                    let raw = quota_global.read().clone();
                                    match parse_bytes(&raw) {
                                        None => ss.set(SaveState::Error("invalid size".to_string())),
                                        Some(v) => {
                                            quota_global.set(format_bytes_hint(v));
                                            let patch = RelayConfigPatch {
                                                quota: Some(QuotaConfigPatch {
                                                    global_max_bytes: Some(v),
                                                    ..Default::default()
                                                }),
                                                ..Default::default()
                                            };
                                            on_patch.call(patch);
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved {
                                                    ss2.set(SaveState::Idle);
                                                }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_quota_global)}
                    }
                }

                // max interfaces per peer
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "MAX INTERFACES" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text",
                            placeholder: "e.g. 50",
                            value: "{quota_ifaces}",
                            oninput: move |e| quota_ifaces.set(e.value()),
                            onblur: {
                                let mut ss = ss_quota_ifaces.clone();
                                move |_| {
                                    let raw = quota_ifaces.read().clone();
                                    match raw.trim().parse::<usize>() {
                                        Err(_) | Ok(0) => ss.set(SaveState::Error("must be > 0".to_string())),
                                        Ok(v) => {
                                            let patch = RelayConfigPatch {
                                                quota: Some(QuotaConfigPatch {
                                                    default_max_interfaces_per_peer: Some(v),
                                                    ..Default::default()
                                                }),
                                                ..Default::default()
                                            };
                                            on_patch.call(patch);
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved {
                                                    ss2.set(SaveState::Idle);
                                                }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_quota_ifaces)}
                    }
                }
            }
        }

        // ── Storage ──────────────────────────────────────────────────────────
        div { class: "relay-panel",
            div { class: "relay-panel-header", "STORAGE" }
            div { class: "relay-panel-body",

                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "DEFAULT TTL (days)" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text",
                            placeholder: "90",
                            value: "{storage_default_ttl}",
                            oninput: move |e| storage_default_ttl.set(e.value()),
                            onblur: {
                                let mut ss = ss_storage_default.clone();
                                move |_| {
                                    let raw = storage_default_ttl.read().clone();
                                    match raw.trim().parse::<u64>() {
                                        Err(_) | Ok(0) => ss.set(SaveState::Error("must be > 0".to_string())),
                                        Ok(v) => {
                                            let patch = RelayConfigPatch {
                                                storage: Some(StorageConfigPatch {
                                                    default_event_ttl_days: Some(v),
                                                    ..Default::default()
                                                }),
                                                ..Default::default()
                                            };
                                            on_patch.call(patch);
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved {
                                                    ss2.set(SaveState::Idle);
                                                }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_storage_default)}
                    }
                }

                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "MAX TTL (days)" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text",
                            placeholder: "365",
                            value: "{storage_max_ttl}",
                            oninput: move |e| storage_max_ttl.set(e.value()),
                            onblur: {
                                let mut ss = ss_storage_max.clone();
                                move |_| {
                                    let raw = storage_max_ttl.read().clone();
                                    match raw.trim().parse::<u64>() {
                                        Err(_) | Ok(0) => ss.set(SaveState::Error("must be > 0".to_string())),
                                        Ok(v) => {
                                            let patch = RelayConfigPatch {
                                                storage: Some(StorageConfigPatch {
                                                    max_event_ttl_days: Some(v),
                                                    ..Default::default()
                                                }),
                                                ..Default::default()
                                            };
                                            on_patch.call(patch);
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved {
                                                    ss2.set(SaveState::Idle);
                                                }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_storage_max)}
                    }
                }

                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "CLEANUP (secs)" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text",
                            placeholder: "3600",
                            value: "{storage_cleanup}",
                            oninput: move |e| storage_cleanup.set(e.value()),
                            onblur: {
                                let mut ss = ss_storage_cleanup.clone();
                                move |_| {
                                    let raw = storage_cleanup.read().clone();
                                    match raw.trim().parse::<u64>() {
                                        Err(_) | Ok(0) => ss.set(SaveState::Error("must be > 0".to_string())),
                                        Ok(v) => {
                                            let patch = RelayConfigPatch {
                                                storage: Some(StorageConfigPatch {
                                                    cleanup_interval_secs: Some(v),
                                                    ..Default::default()
                                                }),
                                                ..Default::default()
                                            };
                                            on_patch.call(patch);
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved {
                                                    ss2.set(SaveState::Idle);
                                                }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_storage_cleanup)}
                    }
                }
            }
        }

        // ── Tiers ────────────────────────────────────────────────────────────
        div { class: "relay-panel",
            div { class: "relay-panel-header", "TIERS · SELF" }
            div { class: "relay-panel-body",
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "MAX BYTES" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text",
                            placeholder: "1 GB",
                            value: "{self_bytes}",
                            oninput: move |e| self_bytes.set(e.value()),
                            onblur: {
                                let mut ss = ss_self_bytes.clone();
                                move |_| {
                                    let raw = self_bytes.read().clone();
                                    match parse_bytes(&raw) {
                                        None => ss.set(SaveState::Error("invalid size".to_string())),
                                        Some(v) => {
                                            self_bytes.set(format_bytes_hint(v));
                                            on_patch.call(RelayConfigPatch {
                                                tiers: Some(TierConfigPatch { self_max_bytes: Some(v), ..Default::default() }),
                                                ..Default::default()
                                            });
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved { ss2.set(SaveState::Idle); }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_self_bytes)}
                    }
                }
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "TTL (days)" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text", placeholder: "365",
                            value: "{self_ttl}",
                            oninput: move |e| self_ttl.set(e.value()),
                            onblur: {
                                let mut ss = ss_self_ttl.clone();
                                move |_| {
                                    match self_ttl.read().trim().parse::<u64>() {
                                        Err(_) | Ok(0) => ss.set(SaveState::Error("must be > 0".to_string())),
                                        Ok(v) => {
                                            on_patch.call(RelayConfigPatch {
                                                tiers: Some(TierConfigPatch { self_ttl_days: Some(v), ..Default::default() }),
                                                ..Default::default()
                                            });
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved { ss2.set(SaveState::Idle); }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_self_ttl)}
                    }
                }
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "MAX INTERFACES" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text", placeholder: "100",
                            value: "{self_ifaces}",
                            oninput: move |e| self_ifaces.set(e.value()),
                            onblur: {
                                let mut ss = ss_self_ifaces.clone();
                                move |_| {
                                    match self_ifaces.read().trim().parse::<usize>() {
                                        Err(_) | Ok(0) => ss.set(SaveState::Error("must be > 0".to_string())),
                                        Ok(v) => {
                                            on_patch.call(RelayConfigPatch {
                                                tiers: Some(TierConfigPatch { self_max_interfaces: Some(v), ..Default::default() }),
                                                ..Default::default()
                                            });
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved { ss2.set(SaveState::Idle); }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_self_ifaces)}
                    }
                }
            }
        }

        div { class: "relay-panel",
            div { class: "relay-panel-header", "TIERS · CONNECTIONS" }
            div { class: "relay-panel-body",
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "MAX BYTES" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text", placeholder: "500 MB",
                            value: "{conn_bytes}",
                            oninput: move |e| conn_bytes.set(e.value()),
                            onblur: {
                                let mut ss = ss_conn_bytes.clone();
                                move |_| {
                                    let raw = conn_bytes.read().clone();
                                    match parse_bytes(&raw) {
                                        None => ss.set(SaveState::Error("invalid size".to_string())),
                                        Some(v) => {
                                            conn_bytes.set(format_bytes_hint(v));
                                            on_patch.call(RelayConfigPatch {
                                                tiers: Some(TierConfigPatch { connections_max_bytes: Some(v), ..Default::default() }),
                                                ..Default::default()
                                            });
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved { ss2.set(SaveState::Idle); }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_conn_bytes)}
                    }
                }
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "TTL (days)" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text", placeholder: "90",
                            value: "{conn_ttl}",
                            oninput: move |e| conn_ttl.set(e.value()),
                            onblur: {
                                let mut ss = ss_conn_ttl.clone();
                                move |_| {
                                    match conn_ttl.read().trim().parse::<u64>() {
                                        Err(_) | Ok(0) => ss.set(SaveState::Error("must be > 0".to_string())),
                                        Ok(v) => {
                                            on_patch.call(RelayConfigPatch {
                                                tiers: Some(TierConfigPatch { connections_ttl_days: Some(v), ..Default::default() }),
                                                ..Default::default()
                                            });
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved { ss2.set(SaveState::Idle); }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_conn_ttl)}
                    }
                }
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "MAX INTERFACES" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text", placeholder: "200",
                            value: "{conn_ifaces}",
                            oninput: move |e| conn_ifaces.set(e.value()),
                            onblur: {
                                let mut ss = ss_conn_ifaces.clone();
                                move |_| {
                                    match conn_ifaces.read().trim().parse::<usize>() {
                                        Err(_) | Ok(0) => ss.set(SaveState::Error("must be > 0".to_string())),
                                        Ok(v) => {
                                            on_patch.call(RelayConfigPatch {
                                                tiers: Some(TierConfigPatch { connections_max_interfaces: Some(v), ..Default::default() }),
                                                ..Default::default()
                                            });
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved { ss2.set(SaveState::Idle); }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_conn_ifaces)}
                    }
                }
            }
        }

        div { class: "relay-panel",
            div { class: "relay-panel-header", "TIERS · PUBLIC" }
            div { class: "relay-panel-body",
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "MAX BYTES" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text", placeholder: "50 MB",
                            value: "{pub_bytes}",
                            oninput: move |e| pub_bytes.set(e.value()),
                            onblur: {
                                let mut ss = ss_pub_bytes.clone();
                                move |_| {
                                    let raw = pub_bytes.read().clone();
                                    match parse_bytes(&raw) {
                                        None => ss.set(SaveState::Error("invalid size".to_string())),
                                        Some(v) => {
                                            pub_bytes.set(format_bytes_hint(v));
                                            on_patch.call(RelayConfigPatch {
                                                tiers: Some(TierConfigPatch { public_max_bytes: Some(v), ..Default::default() }),
                                                ..Default::default()
                                            });
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved { ss2.set(SaveState::Idle); }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_pub_bytes)}
                    }
                }
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "TTL (days)" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text", placeholder: "7",
                            value: "{pub_ttl}",
                            oninput: move |e| pub_ttl.set(e.value()),
                            onblur: {
                                let mut ss = ss_pub_ttl.clone();
                                move |_| {
                                    match pub_ttl.read().trim().parse::<u64>() {
                                        Err(_) | Ok(0) => ss.set(SaveState::Error("must be > 0".to_string())),
                                        Ok(v) => {
                                            on_patch.call(RelayConfigPatch {
                                                tiers: Some(TierConfigPatch { public_ttl_days: Some(v), ..Default::default() }),
                                                ..Default::default()
                                            });
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved { ss2.set(SaveState::Idle); }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_pub_ttl)}
                    }
                }
                div { class: "relay-row admin-field-row",
                    span { class: "relay-row-label", "MAX INTERFACES" }
                    div { class: "admin-field-wrap",
                        input {
                            class: "relay-server-input admin-num-input",
                            r#type: "text", placeholder: "50",
                            value: "{pub_ifaces}",
                            oninput: move |e| pub_ifaces.set(e.value()),
                            onblur: {
                                let mut ss = ss_pub_ifaces.clone();
                                move |_| {
                                    match pub_ifaces.read().trim().parse::<usize>() {
                                        Err(_) | Ok(0) => ss.set(SaveState::Error("must be > 0".to_string())),
                                        Ok(v) => {
                                            on_patch.call(RelayConfigPatch {
                                                tiers: Some(TierConfigPatch { public_max_interfaces: Some(v), ..Default::default() }),
                                                ..Default::default()
                                            });
                                            let mut ss2 = ss.clone();
                                            spawn(async move {
                                                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                                if *ss2.read() == SaveState::Saved { ss2.set(SaveState::Idle); }
                                            });
                                            ss.set(SaveState::Saved);
                                        }
                                    }
                                }
                            },
                        }
                        {save_badge(&ss_pub_ifaces)}
                    }
                }
            }
        }
    }
}

// ── Top-level overlay ─────────────────────────────────────────────────────────

/// Overlay component for viewing and editing relay-node configuration.
///
/// Opened via `state.show_relay_settings = true` (e.g. clicking the relay chip
/// in the status bar). Contains:
/// - P2P relay section (server list, LAN-only toggle, preset)
/// - Relay admin section (fetched from `GET /config`, saved on blur via `PUT /config`)
#[component]
pub fn RelaySettingsOverlay(
    mut state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    if !state.read().show_relay_settings {
        return rsx! {};
    }

    // Resolve the embedded relay service handle reactively from the
    // network signal — returns None until IndrasNode::start finishes.
    let client = move || -> Option<AdminClient> {
        network
            .read()
            .as_ref()
            .and_then(|n| n.relay_service().cloned())
            .map(AdminClient::new)
    };

    // P2P relay local state
    let mut copied = use_signal(|| false);
    let mut ghost_draft = use_signal(String::new);

    let peer_id = "local-peer-id-pending".to_string();
    let peer_id_display = truncate_hex(&peer_id);

    let close = move |_| {
        state.write().show_relay_settings = false;
    };

    let cfg = state.read().relay_config.clone();
    let local_only = cfg.local_only;
    let preset = cfg.preset.clone();
    let servers = cfg.servers.clone();

    // Admin config fetch state
    let mut admin_cfg: Signal<Option<RelayConfigView>> = use_signal(|| None);
    let mut admin_error: Signal<Option<String>> = use_signal(|| None);

    // Fetch config whenever the network signal populates a relay service.
    use_effect(move || {
        let Some(client) = client() else {
            admin_error.set(Some("Relay service not ready yet".to_string()));
            return;
        };
        spawn(async move {
            match client.get_config().await {
                Ok(view) => {
                    admin_cfg.set(Some(view));
                    admin_error.set(None);
                }
                Err(e) => {
                    admin_error.set(Some(e));
                }
            }
        });
    });

    // Callback: apply a patch directly to the embedded relay service
    let on_patch = move |patch: RelayConfigPatch| {
        let Some(client) = client() else {
            admin_error.set(Some("Relay service not ready yet".to_string()));
            return;
        };
        spawn(async move {
            match client.put_config(patch).await {
                Ok(updated) => {
                    admin_cfg.set(Some(updated));
                    admin_error.set(None);
                }
                Err(e) => {
                    admin_error.set(Some(e));
                }
            }
        });
    };

    rsx! {
        div {
            class: "file-modal-overlay",
            onclick: close,
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    state.write().show_relay_settings = false;
                }
            },

            div {
                class: "file-modal relay-settings",
                onclick: move |e| e.stop_propagation(),

                // Header
                div { class: "file-modal-header",
                    div { class: "relay-header-titles",
                        div { class: "relay-eyebrow", "NETWORK · SETTINGS" }
                        div { class: "relay-title", "Relay Node" }
                    }
                    button {
                        class: "file-modal-close",
                        onclick: close,
                        "\u{00d7}"
                    }
                }

                // Body
                div { class: "file-modal-content relay-body",

                    // ── Identity ─────────────────────────────────────────────
                    div { class: "relay-panel",
                        div { class: "relay-panel-header", "IDENTITY" }
                        div { class: "relay-panel-body",
                            div { class: "relay-row",
                                span { class: "relay-row-label", "PEER" }
                                span {
                                    class: "relay-id-value",
                                    title: "Click to copy",
                                    onclick: move |_| {
                                        copied.set(true);
                                        spawn(async move {
                                            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                                            copied.set(false);
                                        });
                                    },
                                    "{peer_id_display}"
                                }
                                if *copied.read() {
                                    span { class: "relay-copied-flash", "copied" }
                                }
                            }

                            // Preset selector — segmented pills
                            div { class: "relay-row relay-preset-row",
                                span { class: "relay-row-label", "PRESET" }
                                div { class: "relay-preset-group",
                                    for name in PRESET_NAMES.iter() {
                                        {
                                            let is_active = preset == *name;
                                            let n = (*name).to_string();
                                            rsx! {
                                                button {
                                                    key: "{name}",
                                                    class: if is_active { "relay-preset-pill active" } else { "relay-preset-pill" },
                                                    onclick: move |_| {
                                                        state.write().relay_config.preset = n.clone();
                                                        persist(&state);
                                                    },
                                                    "{name}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ── Relay mode toggle ────────────────────────────────────
                    div { class: "relay-panel",
                        div { class: "relay-panel-body",
                            div { class: "relay-row relay-toggle-row",
                                div { class: "relay-toggle-text",
                                    div { class: "relay-toggle-label", "Use public relays" }
                                    div { class: "relay-toggle-hint",
                                        if local_only {
                                            "LAN-only: peers must be on the same network"
                                        } else {
                                            "Public relays enable peers to find each other anywhere"
                                        }
                                    }
                                }
                                button {
                                    class: if !local_only { "relay-toggle on" } else { "relay-toggle" },
                                    onclick: move |_| {
                                        let new_val = !state.read().relay_config.local_only;
                                        state.write().relay_config.local_only = new_val;
                                        persist(&state);
                                    },
                                    span { class: "relay-toggle-knob" }
                                }
                            }
                            div { class: "relay-restart-note",
                                span { class: "relay-restart-dot" }
                                "active on restart"
                            }
                        }
                    }

                    // ── Relay servers list ───────────────────────────────────
                    div { class: "relay-panel",
                        div { class: "relay-panel-header", "RELAY SERVERS" }
                        div { class: "relay-panel-body relay-server-list",
                            for (idx, url) in servers.iter().enumerate() {
                                {
                                    let url_owned = url.clone();
                                    rsx! {
                                        div {
                                            key: "{idx}",
                                            class: "relay-server-row",
                                            input {
                                                class: "relay-server-input",
                                                r#type: "text",
                                                value: "{url_owned}",
                                                onchange: move |e| {
                                                    let v = e.value().trim().to_string();
                                                    if v.is_empty() {
                                                        state.write().relay_config.servers.remove(idx);
                                                    } else {
                                                        state.write().relay_config.servers[idx] = v;
                                                    }
                                                    persist(&state);
                                                },
                                            }
                                            button {
                                                class: "relay-server-remove",
                                                title: "Remove relay",
                                                onclick: move |_| {
                                                    state.write().relay_config.servers.remove(idx);
                                                    persist(&state);
                                                },
                                                "\u{00d7}"
                                            }
                                        }
                                    }
                                }
                            }

                            // Ghost-add row
                            div { class: "relay-server-row relay-server-ghost-row",
                                input {
                                    class: "relay-server-input relay-server-ghost",
                                    r#type: "text",
                                    placeholder: "+ add relay server",
                                    value: "{ghost_draft}",
                                    oninput: move |e| ghost_draft.set(e.value()),
                                    onchange: move |e| {
                                        let v = e.value().trim().to_string();
                                        if !v.is_empty() {
                                            state.write().relay_config.servers.push(v);
                                            persist(&state);
                                            ghost_draft.set(String::new());
                                        }
                                    },
                                }
                            }
                        }
                    }

                    // Status footer
                    div { class: "relay-status-footer",
                        if servers.is_empty() {
                            span { class: "relay-status-empty", "No relays configured" }
                        } else {
                            for (idx, url) in servers.iter().enumerate() {
                                div {
                                    key: "{idx}",
                                    class: "relay-status-line",
                                    span { class: "relay-status-dot" }
                                    span { class: "relay-status-url", "{url}" }
                                }
                            }
                        }
                    }

                    // ── Admin config divider ─────────────────────────────────
                    div { class: "relay-panel-divider",
                        span { class: "relay-panel-divider-label", "RELAY ADMIN CONFIG" }
                    }

                    // Admin section: loading / error / form
                    if let Some(err) = admin_error.read().clone() {
                        div { class: "relay-panel",
                            div { class: "relay-panel-body",
                                div { class: "admin-fetch-error",
                                    "Could not reach relay admin API: {err}"
                                }
                                div { class: "admin-fetch-hint",
                                    "The embedded relay is still starting — try again in a moment."
                                }
                            }
                        }
                    } else if let Some(view) = admin_cfg.read().clone() {
                        AdminConfigPanel {
                            cfg: view,
                            on_patch: on_patch,
                        }
                    } else {
                        div { class: "relay-panel",
                            div { class: "relay-panel-body",
                                div { class: "admin-loading", "Loading admin config…" }
                            }
                        }
                    }
                }
            }
        }
    }
}
