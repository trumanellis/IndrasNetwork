//! Per-agent stage view — which files are tracked in an agent's local
//! working-tree index right now (and would land in a `Changeset` if
//! Commit were pressed).
//!
//! Read-only presentation. Snapshot is passed in by the parent panel so
//! this component stays stateless and easy to test.

use dioxus::prelude::*;
use indras_sync_engine::braid::PatchFile;
use indras_sync_engine::team::LogicalAgentId;

/// Per-agent stage view. Lists tracked files with short hash + byte size.
///
/// Empty state renders "No pending changes" so the user sees the agent
/// even when its working tree is clean.
#[component]
pub fn SyncStageView(agent: LogicalAgentId, files: Vec<PatchFile>) -> Element {
    rsx! {
        div { class: "sync-stage-view",
            div { class: "sync-stage-header",
                span { class: "sync-stage-agent", "{agent}" }
                span { class: "sync-stage-count", "{files.len()} file(s)" }
            }
            if files.is_empty() {
                div { class: "sync-stage-empty", "No pending changes" }
            } else {
                div { class: "sync-stage-items",
                    for file in files {
                        div { class: "sync-stage-item",
                            span { class: "sync-stage-path", "{file.path}" }
                            span { class: "sync-stage-hash", "{short_hash(&file.hash)}" }
                            span { class: "sync-stage-size", "{format_size(file.size)}" }
                        }
                    }
                }
            }
        }
    }
}

/// Render the first 4 bytes of a BLAKE3 hash as 8 hex chars, matching the
/// abbreviation style used by `ContentRef::short_hash`.
fn short_hash(hash: &[u8; 32]) -> String {
    let mut s = String::with_capacity(8);
    for b in &hash[..4] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Compact file size for display: "1.2 KB", "340 B", "5.3 MB".
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_hash_renders_eight_hex_chars() {
        let mut h = [0u8; 32];
        h[0] = 0xab;
        h[1] = 0xcd;
        h[2] = 0xef;
        h[3] = 0x12;
        assert_eq!(short_hash(&h), "abcdef12");
    }

    #[test]
    fn format_size_boundaries() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0 GB");
    }
}
