//! Mobile bottom navigation bar.

use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum NavTab {
    Vault,
    Stories,
    Artifacts,
    Quests,
    Profile,
}

#[component]
pub fn BottomNav(
    active_tab: NavTab,
    on_tab_change: EventHandler<NavTab>,
) -> Element {
    let tabs = vec![
        (NavTab::Vault, "\u{1F310}", "Vault"),
        (NavTab::Stories, "\u{1F4AC}", "Stories"),
        (NavTab::Artifacts, "\u{1F4E6}", "Artifacts"),
        (NavTab::Quests, "\u{2694}", "Quests"),
        (NavTab::Profile, "\u{1F464}", "Profile"),
    ];

    rsx! {
        div {
            class: "bottom-nav",
            div {
                class: "bottom-nav-inner",
                for (tab, icon, label) in tabs.iter() {
                    {
                        let is_active = *tab == active_tab;
                        let item_class = if is_active { "bottom-nav-item active" } else { "bottom-nav-item" };
                        let tab_clone = tab.clone();
                        rsx! {
                            button {
                                class: "{item_class}",
                                onclick: move |_| on_tab_change.call(tab_clone.clone()),
                                span { class: "nav-icon", "{icon}" }
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}
