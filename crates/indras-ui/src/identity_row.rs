//! Identity display row with avatar, name, and short ID.

use dioxus::prelude::*;

/// Displays a user's identity with avatar letter, name, and short bech32m ID.
#[component]
pub fn IdentityRow(
    avatar_letter: String,
    name: String,
    short_id: String,
) -> Element {
    rsx! {
        div {
            class: "identity-row",
            div { class: "identity-avatar", "{avatar_letter}" }
            div {
                div { class: "identity-name", "{name}" }
                div { class: "identity-id", "{short_id}" }
            }
        }
    }
}
