//! 4-column Finder-inspired vault layout.

use dioxus::prelude::*;

use crate::state::{AppState, RealmCategory};
use super::private_column::PrivateColumn;
use super::realm_column::RealmColumn;

/// The 4-column grid: Private | DMs | Groups | Public.
#[component]
pub fn VaultColumns(state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "vault-columns",
            PrivateColumn { state }
            RealmColumn { state, category: RealmCategory::Dm, label: "DMs" }
            RealmColumn { state, category: RealmCategory::Group, label: "GROUPS" }
            RealmColumn { state, category: RealmCategory::Public, label: "PUBLIC" }
        }
    }
}
