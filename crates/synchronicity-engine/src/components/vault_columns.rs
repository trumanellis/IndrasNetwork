//! 4-column Finder-inspired vault layout.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use crate::state::{AppState, RealmCategory};
use super::private_column::PrivateColumn;
use super::realm_column::RealmColumn;

/// The 4-column grid: Private | DMs | Groups | World.
#[component]
pub fn VaultColumns(
    state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
) -> Element {
    rsx! {
        div { class: "vault-columns",
            PrivateColumn { state }
            RealmColumn { state, network, category: RealmCategory::Dm, label: "CONNECTIONS" }
            RealmColumn { state, network, category: RealmCategory::Group, label: "GROUPS" }
            RealmColumn { state, network, category: RealmCategory::World, label: "WORLD" }
        }
    }
}
