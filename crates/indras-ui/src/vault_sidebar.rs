//! Vault tree sidebar with expandable sections and heat indicators.

use dioxus::prelude::*;

/// A node in the vault tree for sidebar display.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub heat_level: u8,
    pub depth: usize,
    pub has_children: bool,
    pub expanded: bool,
    pub active: bool,
    pub section: Option<String>,
    pub view_type: String,
}

/// Vault sidebar tree with sections, expand/collapse, heat dots.
#[component]
pub fn VaultSidebar(
    nodes: Vec<TreeNode>,
    on_click: EventHandler<String>,
    on_toggle: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            class: "vault-tree",
            for node in nodes.iter() {
                if let Some(ref section) = node.section {
                    div { class: "tree-section-label", "{section}" }
                }
                {
                    let active_class = if node.active { " active" } else { "" };
                    let heat_attr = node.heat_level.to_string();
                    let node_id = node.id.clone();
                    let node_id2 = node.id.clone();
                    rsx! {
                        div {
                            class: "tree-item{active_class}",
                            "data-heat": "{heat_attr}",
                            onclick: move |_| on_click.call(node_id.clone()),
                            // Indentation
                            for _ in 0..node.depth {
                                div { class: "tree-indent" }
                            }
                            // Toggle arrow (if has children)
                            if node.has_children {
                                div {
                                    class: if node.expanded { "tree-toggle open" } else { "tree-toggle" },
                                    onclick: move |evt| {
                                        evt.stop_propagation();
                                        on_toggle.call(node_id2.clone());
                                    },
                                    "▶"
                                }
                            } else {
                                div { class: "tree-toggle" }
                            }
                            div { class: "tree-icon", "{node.icon}" }
                            div { class: "tree-label", "{node.label}" }
                            div { class: "heat-dot heat-{node.heat_level}" }
                        }
                    }
                }
            }
        }
    }
}
