//! Global app state using Dioxus signals.

use std::collections::HashMap;
use std::sync::Arc;
use dioxus::prelude::*;
use indras_network::RealmId;
use crate::bridge::NetworkHandle;

/// Top-level app phase.
#[derive(Clone, Debug, PartialEq)]
pub enum AppPhase {
    /// Checking if first run
    Loading,
    /// Onboarding flow
    Setup,
    /// Main chat view
    Running(Arc<NetworkHandle>),
}

/// A snapshot of a system event for inline display.
#[derive(Clone, Debug)]
pub struct SystemEventSnapshot {
    pub id: String,
    pub text: String,
    pub timestamp: u64,
}

/// Shared chat state provided via Dioxus context.
#[derive(Clone, Copy)]
pub struct ChatContext {
    pub handle: Signal<Arc<NetworkHandle>>,
    pub active_chat: Signal<Option<RealmId>>,
    pub conversations: Signal<Vec<ConversationSummary>>,
    pub show_add_contact: Signal<bool>,
    /// Display names of peers currently typing in the active chat.
    pub typing_peers: Signal<Vec<String>>,
    /// Persistent system events per realm (survives chat switching).
    pub system_events: Signal<HashMap<RealmId, Vec<SystemEventSnapshot>>>,
}

/// Summary of a conversation for the sidebar.
#[derive(Clone, Debug)]
pub struct ConversationSummary {
    pub realm_id: RealmId,
    pub display_name: String,
    pub last_message: Option<String>,
    pub last_message_time: Option<u64>,
    pub unread_count: u32,
}
