//! Network event subscription loop.
//!
//! Subscribes to the global network event stream and converts raw events
//! into human-readable log messages for the workspace event log.

use std::sync::Arc;
use futures::StreamExt;
use indras_network::IndrasNetwork;

/// Subscribe to network events and return log messages via callback.
///
/// This function runs indefinitely, calling `on_event` for each network event
/// with a formatted log message string. It should be spawned as a background
/// task.
///
/// The callback receives a formatted message like `[realm_id] EventType`.
pub async fn subscribe_network_events<F>(network: Arc<IndrasNetwork>, mut on_event: F)
where
    F: FnMut(String),
{
    let mut events = std::pin::pin!(network.events());
    while let Some(event) = events.next().await {
        let description = format!("{:?}", event.event.event);
        // Extract just the variant name (e.g. "Message", "MembershipChange", "Presence")
        let event_type = description.split_once('{')
            .or_else(|| description.split_once('('))
            .map(|(prefix, _)| prefix.trim())
            .unwrap_or(&description);

        let realm_short = format!("{}", event.realm_id);
        let msg = format!("[{}] {}", &realm_short[..8.min(realm_short.len())], event_type);

        on_event(msg);
    }
}
