//! Channel types for Lua thread ↔ Dioxus main thread communication.

use tokio::sync::{broadcast, mpsc, oneshot};

use super::action::Action;
use super::event::AppEvent;
use super::query::{Query, QueryResult};

/// Channels shared between the Lua thread and the Dioxus app.
/// Provided to Dioxus via context.
pub struct AppTestChannels {
    /// Receive actions from Lua (Lua sends, Dioxus receives)
    pub action_rx: mpsc::Receiver<Action>,
    /// Send events to Lua (Dioxus sends, Lua subscribes)
    pub event_tx: broadcast::Sender<AppEvent>,
    /// Receive queries from Lua (Lua sends query + oneshot, Dioxus replies)
    pub query_rx: mpsc::Receiver<(Query, oneshot::Sender<QueryResult>)>,
}

/// The Lua-side channel handles.
pub struct LuaChannels {
    /// Send actions to Dioxus
    pub action_tx: mpsc::Sender<Action>,
    /// Receive events from Dioxus
    pub event_rx: broadcast::Receiver<AppEvent>,
    /// Send queries to Dioxus (with oneshot for reply)
    pub query_tx: mpsc::Sender<(Query, oneshot::Sender<QueryResult>)>,
}

/// Create all channels and return both sides.
pub fn create_test_channels() -> (AppTestChannels, LuaChannels) {
    let (action_tx, action_rx) = mpsc::channel(256);
    let (event_tx, event_rx) = broadcast::channel(256);
    let (query_tx, query_rx) = mpsc::channel(64);

    let app_channels = AppTestChannels {
        action_rx,
        event_tx: event_tx.clone(),
        query_rx,
    };

    let lua_channels = LuaChannels {
        action_tx,
        event_rx,
        query_tx,
    };

    (app_channels, lua_channels)
}
