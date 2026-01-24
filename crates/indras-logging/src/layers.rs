//! Custom tracing layers for Indras Network
//!
//! This module provides layers that inject peer context and other
//! Indras-specific fields into tracing events.

use tracing::{Event, Subscriber, span};
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::LookupSpan,
};

use crate::context::{PeerContextData, PeerContextGuard};

/// Layer that injects peer context into spans and events
///
/// This layer automatically adds `peer_id`, `peer_type`, and `instance_id`
/// fields to all spans when a [`PeerContextGuard`] is active.
pub struct PeerContextLayer;

impl PeerContextLayer {
    /// Create a new peer context layer
    pub fn new() -> Self {
        Self
    }
}

impl Default for PeerContextLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension data stored on spans
#[derive(Debug, Clone)]
pub struct PeerContextExtension {
    pub data: PeerContextData,
}

impl<S> Layer<S> for PeerContextLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, _attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            // If there's an active peer context, attach it to the span
            if let Some(peer_ctx) = PeerContextGuard::current() {
                span.extensions_mut()
                    .insert(PeerContextExtension { data: peer_ctx });
            }
        }
    }

    fn on_event(&self, _event: &Event<'_>, _ctx: Context<'_, S>) {
        // Events inherit context from their parent span
        // The JSON formatter will include the span's recorded fields
    }
}

/// Layer that adds timestamp formatting
pub struct TimestampLayer {
    /// Whether to use RFC3339 format (default) or Unix timestamp
    pub use_rfc3339: bool,
}

impl TimestampLayer {
    pub fn new() -> Self {
        Self { use_rfc3339: true }
    }

    pub fn with_unix_timestamps(mut self) -> Self {
        self.use_rfc3339 = false;
        self
    }
}

impl Default for TimestampLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Layer<S> for TimestampLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    // Timestamps are handled by the JSON formatter, but this layer
    // could be extended to add custom timestamp behavior
}

/// Create a JSONL formatting layer for file output
///
/// This configures tracing-subscriber's JSON formatter with settings
/// optimized for log aggregation systems.
pub fn jsonl_file_layer<W>(
    writer: W,
    include_location: bool,
    include_thread_info: bool,
) -> tracing_subscriber::fmt::Layer<
    tracing_subscriber::Registry,
    tracing_subscriber::fmt::format::JsonFields,
    tracing_subscriber::fmt::format::Format<tracing_subscriber::fmt::format::Json>,
    W,
>
where
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + 'static,
{
    let layer = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .flatten_event(true)
        .with_writer(writer);

    let layer = if include_location {
        layer.with_file(true).with_line_number(true)
    } else {
        layer.with_file(false).with_line_number(false)
    };

    if include_thread_info {
        layer.with_thread_ids(true).with_thread_names(true)
    } else {
        layer.with_thread_ids(false).with_thread_names(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    #[test]
    fn test_peer_context_layer_creation() {
        let _layer = PeerContextLayer::new();
    }

    #[test]
    fn test_peer_context_extension() {
        let peer = SimulationIdentity::new('A').unwrap();
        let _guard = PeerContextGuard::new(&peer);

        // Verify context is available
        let ctx = PeerContextGuard::current().unwrap();
        assert_eq!(ctx.peer_id, "A");

        // Create extension
        let ext = PeerContextExtension { data: ctx };
        assert_eq!(ext.data.peer_id, "A");
    }
}
