//! 4-column Finder-inspired vault layout.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use crate::state::{AppState, RealmCategory};
use crate::vault_manager::VaultManager;
use super::private_column::PrivateColumn;
use super::realm_column::RealmColumn;

/// The 4-column grid: Private | DMs | Groups | World.
#[component]
pub fn VaultColumns(
    state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
) -> Element {
    rsx! {
        div { class: "vault-columns",
            PrivateColumn { state }
            RealmColumn { state, network, vault_manager, category: RealmCategory::Dm, label: "CONNECTIONS" }
            RealmColumn { state, network, vault_manager, category: RealmCategory::Group, label: "GROUPS" }
            RealmColumn { state, network, vault_manager, category: RealmCategory::World, label: "WORLD" }
        }
    }
}
