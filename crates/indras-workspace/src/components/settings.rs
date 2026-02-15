//! Settings view with identity display and theme switcher.

use dioxus::prelude::*;
use indras_ui::ThemeSwitcher;

#[component]
pub fn SettingsView(
    player_name: String,
    player_letter: String,
    player_short_id: String,
) -> Element {
    rsx! {
        div {
            class: "view active",
            div {
                class: "content-scroll",
                div {
                    class: "content-body",
                    div { class: "doc-title", "Settings" }

                    div {
                        class: "settings-section",
                        div { class: "settings-section-title", "Identity" }
                        div {
                            class: "settings-identity",
                            div {
                                class: "settings-identity-avatar",
                                "{player_letter}"
                            }
                            div {
                                class: "settings-identity-info",
                                div { class: "settings-identity-name", "{player_name}" }
                                div { class: "settings-identity-id", "{player_short_id}" }
                            }
                        }
                    }

                    div {
                        class: "settings-section",
                        div { class: "settings-section-title", "Appearance" }
                        ThemeSwitcher {}
                    }
                }
            }
        }
    }
}
