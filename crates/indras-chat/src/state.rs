//! Global app state using Dioxus signals.

use std::collections::HashMap;
use std::sync::Arc;
use dioxus::prelude::*;
use indras_network::{IndrasNetwork, PeerInfo, RealmId};

/// Top-level app phase.
#[derive(Clone)]
pub enum AppPhase {
    /// Checking if first run
    Loading,
    /// Onboarding flow
    Setup,
    /// Main chat view
    Running(Arc<IndrasNetwork>),
}

impl std::fmt::Debug for AppPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loading => write!(f, "Loading"),
            Self::Setup => write!(f, "Setup"),
            Self::Running(_) => write!(f, "Running(..)"),
        }
    }
}

impl PartialEq for AppPhase {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Loading, Self::Loading) => true,
            (Self::Setup, Self::Setup) => true,
            (Self::Running(a), Self::Running(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
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
    pub runtime: Signal<Arc<IndrasNetwork>>,
    pub active_chat: Signal<Option<RealmId>>,
    pub conversations: Signal<Vec<ConversationSummary>>,
    pub peers: Signal<Vec<PeerInfo>>,
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
