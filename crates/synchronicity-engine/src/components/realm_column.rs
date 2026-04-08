//! Realm column — accordion list of realms for a given category.

use dioxus::prelude::*;

use crate::state::{AppState, ModalFile, RealmCategory, RealmId as _};
use super::file_item::FileItem;

/// A column showing realms of a specific category with accordion file lists.
#[component]
pub fn RealmColumn(
    mut state: Signal<AppState>,
    category: RealmCategory,
    label: &'static str,
) -> Element {
    let realms: Vec<_> = state.read().realms.iter()
        .filter(|r| r.category == category)
        .cloned()
        .collect();
    let expanded = state.read().selection.expanded_realms.clone();
    let selected_realm = state.read().selection.selected_realm;
    let selected_file = state.read().selection.selected_file.clone();

    rsx! {
        div { class: "vault-column",
            div { class: "column-header", "{label}" }
            div { class: "vault-column-body",
                if realms.is_empty() {
                    {
                        let (empty_icon, empty_text) = match category {
                            RealmCategory::Dm => ("💬", "Connect with someone to start a conversation"),
                            RealmCategory::Group => ("👥", "Join or create a group to collaborate"),
                            RealmCategory::Public => ("🌍", "Public realms will appear here"),
                            RealmCategory::Private => ("🏠", "Your private vault is empty"),
                        };
                        let is_dm = category == RealmCategory::Dm;
                        rsx! {
                            div { class: "column-empty",
                                div { class: "column-empty-icon", "{empty_icon}" }
                                div { class: "column-empty-text", "{empty_text}" }
                                if is_dm {
                                    button {
                                        class: "se-btn-outline se-btn-sm",
                                        onclick: move |_| {
                                            state.write().show_contact_invite = true;
                                        },
                                        "Add Contact"
                                    }
                                }
                            }
                        }
                    }
                } else {
                    for realm in realms {
                        {
                            let id = realm.id;
                            let is_expanded = expanded.contains(&id);
                            let is_selected = selected_realm == Some(id);
                            let chevron_class = if is_expanded { "realm-chevron expanded" } else { "realm-chevron" };
                            let entry_class = if is_selected { "realm-entry selected" } else { "realm-entry" };
                            let files_class = if is_expanded { "realm-files expanded" } else { "realm-files" };

                            rsx! {
                                // Realm header — click to expand/collapse
                                div {
                                    class: "{entry_class}",
                                    onclick: move |_| {
                                        let mut sel = state.read().selection.clone();
                                        if sel.expanded_realms.contains(&id) {
                                            sel.expanded_realms.remove(&id);
                                        } else {
                                            sel.expanded_realms.insert(id);
                                        }
                                        state.write().selection = sel;
                                    },
                                    span { class: "{chevron_class}", "\u{25B8}" }
                                    span { class: "realm-entry-name", "{realm.display_name}" }
                                    span { class: "realm-entry-meta", "{realm.member_count}" }
                                }

                                // Accordion file list
                                div {
                                    class: "{files_class}",
                                    for file in &realm.files {
                                        {
                                            let path = file.path.clone();
                                            let is_sel = is_selected && selected_file.as_deref() == Some(path.as_str());
                                            let file = file.clone();
                                            rsx! {
                                                FileItem {
                                                    file: file,
                                                    is_selected: is_sel,
                                                    on_click: move |p: String| {
                                                        state.write().selection.selected_realm = Some(id);
                                                        state.write().selection.selected_file = Some(p.clone());
                                                        state.write().modal_file = Some(ModalFile {
                                                            realm_id: Some(id),
                                                            file_path: p,
                                                        });
                                                    },
                                                }
                                            }
                                        }
                                    }
                                    if realm.files.is_empty() && is_expanded {
                                        div { class: "realm-files-empty", "No files" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
