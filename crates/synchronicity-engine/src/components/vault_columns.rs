//! 4-column Finder-inspired vault layout.

use std::sync::Arc;

use dioxus::prelude::*;
use indras_network::IndrasNetwork;

use crate::state::{AppState, PeerDisplayInfo, RealmCategory};
use crate::vault_manager::VaultManager;
use super::private_column::PrivateColumn;
use super::realm_column::RealmColumn;

/// The 4-column grid: Private | DMs | Groups | World.
#[component]
pub fn VaultColumns(
    state: Signal<AppState>,
    network: Signal<Option<Arc<IndrasNetwork>>>,
    vault_manager: Signal<Option<Arc<VaultManager>>>,
    peers: Signal<Vec<PeerDisplayInfo>>,
) -> Element {
    rsx! {
        div { class: "vault-columns",
            PrivateColumn { state }
            RealmColumn { state, network, vault_manager, peers, category: RealmCategory::Dm, label: "CONNECTIONS" }
            RealmColumn { state, network, vault_manager, peers, category: RealmCategory::Group, label: "GROUPS" }
            RealmColumn { state, network, vault_manager, peers, category: RealmCategory::World, label: "WORLD" }
        }
    }
}
