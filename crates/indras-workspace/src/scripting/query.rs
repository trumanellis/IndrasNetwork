//! Query types for synchronous state inspection from Lua scripts.

/// Queries that Lua can send to inspect app state.
#[derive(Debug, Clone)]
pub enum Query {
    Identity,
    AppPhase,
    ActiveView,
    ActiveSidebarItem,
    PeerCount,
    PeerNames,
    ChatMessageCount,
    ChatMessages,
    SidebarItems,
    EventLog,
    OverlayOpen,
    Custom(String),
}

impl Query {
    /// Parse a query name string into a Query enum.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "identity" => Ok(Query::Identity),
            "app_phase" => Ok(Query::AppPhase),
            "active_view" => Ok(Query::ActiveView),
            "active_sidebar_item" => Ok(Query::ActiveSidebarItem),
            "peer_count" => Ok(Query::PeerCount),
            "peer_names" => Ok(Query::PeerNames),
            "chat_message_count" => Ok(Query::ChatMessageCount),
            "chat_messages" => Ok(Query::ChatMessages),
            "sidebar_items" => Ok(Query::SidebarItems),
            "event_log" => Ok(Query::EventLog),
            "overlay_open" => Ok(Query::OverlayOpen),
            other => Ok(Query::Custom(other.to_string())),
        }
    }
}

/// Results returned from queries.
#[derive(Debug, Clone)]
pub enum QueryResult {
    String(String),
    Number(f64),
    StringList(Vec<String>),
    Json(serde_json::Value),
    Error(String),
}
