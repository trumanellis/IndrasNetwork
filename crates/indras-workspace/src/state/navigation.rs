//! Navigation state — vault tree, breadcrumbs, expand/collapse.

use indras_artifacts::{ArtifactId, TreeType};
use std::collections::HashSet;

/// A breadcrumb entry in the navigation trail.
#[derive(Clone, Debug, PartialEq)]
pub struct BreadcrumbEntry {
    pub id: String,
    pub label: String,
}

/// A node in the flattened vault tree for sidebar display.
#[derive(Clone, Debug, PartialEq)]
pub struct VaultTreeNode {
    pub id: String,
    pub artifact_id: Option<ArtifactId>,
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

/// Navigation state for the workspace.
#[derive(Clone, Debug)]
pub struct NavigationState {
    pub breadcrumbs: Vec<BreadcrumbEntry>,
    pub current_id: Option<String>,
    pub expanded_nodes: HashSet<String>,
    pub vault_tree: Vec<VaultTreeNode>,
}

impl NavigationState {
    pub fn new() -> Self {
        Self {
            breadcrumbs: vec![BreadcrumbEntry {
                id: "root".to_string(),
                label: "Vault".to_string(),
            }],
            current_id: None,
            expanded_nodes: HashSet::new(),
            vault_tree: Vec::new(),
        }
    }

    /// Navigate to an artifact, updating breadcrumbs.
    pub fn navigate_to(&mut self, id: String, label: String) {
        // Add to breadcrumbs (simplified - just append)
        self.current_id = Some(id.clone());

        // Check if already in breadcrumbs
        if let Some(pos) = self.breadcrumbs.iter().position(|b| b.id == id) {
            // Truncate to this position
            self.breadcrumbs.truncate(pos + 1);
        } else {
            self.breadcrumbs.push(BreadcrumbEntry { id, label });
        }

        // Mark the node as active in the tree
        for node in &mut self.vault_tree {
            node.active = self.current_id.as_ref() == Some(&node.id);
        }
    }

    /// Toggle expand/collapse of a tree node.
    pub fn toggle_expand(&mut self, id: &str) {
        if self.expanded_nodes.contains(id) {
            self.expanded_nodes.remove(id);
        } else {
            self.expanded_nodes.insert(id.to_string());
        }
    }

    /// Get icon for a tree type.
    pub fn icon_for_tree_type(tree_type: &TreeType) -> &'static str {
        match tree_type {
            TreeType::Vault => "🌐",
            TreeType::Story => "💬",
            TreeType::Gallery => "🎨",
            TreeType::Document => "📄",
            TreeType::Request => "📋",
            TreeType::Exchange => "🔄",
            TreeType::Collection => "📚",
            TreeType::Inbox => "📥",
            TreeType::Quest => "⚔",
            TreeType::Need => "🌱",
            TreeType::Offering => "🎁",
            TreeType::Intention => "✨",
            TreeType::Custom(_) => "📦",
        }
    }

    /// Get view type string for a tree type.
    pub fn view_type_for_tree(tree_type: &TreeType) -> &'static str {
        match tree_type {
            TreeType::Story => "story",
            TreeType::Quest | TreeType::Need | TreeType::Offering | TreeType::Intention => "quest",
            _ => "document",
        }
    }
}
