//! Navigation hub sidebar — replaces vault tree with Navigate / Create / Recent sections.

use dioxus::prelude::*;

/// Where the user wants to navigate.
#[derive(Clone, Debug, PartialEq)]
pub enum NavDestination {
    Home,
    Artifacts,
    Contacts,
    Quests,
    Settings,
}

/// Artifact creation action.
#[derive(Clone, Debug, PartialEq)]
pub enum CreateAction {
    Document,
    Story,
    Quest,
    Need,
    Offering,
    Intention,
}

/// A recently-accessed artifact for quick navigation.
#[derive(Clone, Debug, PartialEq)]
pub struct RecentItem {
    pub id: String,
    pub label: String,
    pub icon: String,
}

/// Navigation hub sidebar with Navigate, Create, and Recent sections.
#[component]
pub fn NavigationSidebar(
    active: NavDestination,
    recent_items: Vec<RecentItem>,
    on_navigate: EventHandler<NavDestination>,
    on_create: EventHandler<CreateAction>,
    on_recent_click: EventHandler<String>,
) -> Element {
    let nav_items: Vec<(NavDestination, &str, &str)> = vec![
        (NavDestination::Home, "📄", "Home"),
        (NavDestination::Artifacts, "📦", "Artifacts"),
        (NavDestination::Contacts, "👤", "Contacts"),
        (NavDestination::Quests, "⚔", "Quests"),
        (NavDestination::Settings, "⚙", "Settings"),
    ];

    let create_items: Vec<(CreateAction, &str, &str)> = vec![
        (CreateAction::Document, "📄", "Document"),
        (CreateAction::Story, "💬", "Story"),
        (CreateAction::Quest, "⚔", "Quest"),
        (CreateAction::Need, "🌱", "Need"),
        (CreateAction::Offering, "🎁", "Offering"),
        (CreateAction::Intention, "✨", "Intention"),
    ];

    rsx! {
        div {
            class: "vault-tree",

            // --- NAVIGATE ---
            div { class: "tree-section-label", "NAVIGATE" }
            for (dest, icon, label) in nav_items.iter() {
                {
                    let is_active = *dest == active;
                    let item_class = if is_active { "tree-item active" } else { "tree-item" };
                    let dest_clone = dest.clone();
                    rsx! {
                        div {
                            class: "{item_class}",
                            onclick: move |_| on_navigate.call(dest_clone.clone()),
                            div { class: "tree-icon", "{icon}" }
                            div { class: "tree-label", "{label}" }
                        }
                    }
                }
            }

            // --- CREATE ---
            div { class: "tree-section-label", "CREATE" }
            for (action, icon, label) in create_items.iter() {
                {
                    let action_clone = action.clone();
                    rsx! {
                        div {
                            class: "tree-item",
                            onclick: move |_| on_create.call(action_clone.clone()),
                            div { class: "tree-icon", "{icon}" }
                            div { class: "tree-label", "{label}" }
                        }
                    }
                }
            }

            // --- RECENT ---
            if !recent_items.is_empty() {
                div { class: "tree-section-label", "RECENT" }
                for item in recent_items.iter() {
                    {
                        let item_id = item.id.clone();
                        rsx! {
                            div {
                                class: "tree-item",
                                onclick: move |_| on_recent_click.call(item_id.clone()),
                                div { class: "tree-icon", "{item.icon}" }
                                div { class: "tree-label", "{item.label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
