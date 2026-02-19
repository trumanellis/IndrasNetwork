//! Channel types for Lua thread ↔ Dioxus main thread communication.

use tokio::sync::{broadcast, mpsc, oneshot};

use super::action::Action;
use super::event::AppEvent;
use super::query::{Query, QueryResult};

/// Channels shared between the Lua thread and the Dioxus app.
/// Provided to Dioxus via context.
///
/// The receivers are wrapped in `Option` so they can be moved out
/// (via `.take()`) into separate dispatcher tasks, avoiding a shared
/// mutex that would deadlock when one dispatcher holds the lock while
/// the other needs it.
pub struct AppTestChannels {
    /// Receive actions from Lua (Lua sends, Dioxus receives).
    /// Taken by the action dispatcher at startup.
    pub action_rx: Option<mpsc::Receiver<Action>>,
    /// Send events to Lua (Dioxus sends, Lua subscribes)
    pub event_tx: broadcast::Sender<AppEvent>,
    /// Receive queries from Lua (Lua sends query + oneshot, Dioxus replies).
    /// Taken by the query dispatcher at startup.
    pub query_rx: Option<mpsc::Receiver<(Query, oneshot::Sender<QueryResult>)>>,
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
        action_rx: Some(action_rx),
        event_tx: event_tx.clone(),
        query_rx: Some(query_rx),
    };

    let lua_channels = LuaChannels {
        action_tx,
        event_rx,
        query_tx,
    };

    (app_channels, lua_channels)
}
