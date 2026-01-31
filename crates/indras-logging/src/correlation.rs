//! Correlation ID system for distributed tracing
//!
//! This module provides correlation contexts that can be attached to packets
//! and propagated across peer instances, enabling end-to-end tracing of
//! message flows through the network.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Correlation context for distributed tracing
///
/// This context is designed to be propagated with packets through the network,
/// allowing logs from different peer instances to be correlated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationContext {
    /// Root trace ID - same across all peers for one packet journey
    ///
    /// This ID is generated when a message is first created and remains
    /// constant as the packet traverses the network.
    pub trace_id: Uuid,

    /// Span ID - unique to this specific operation
    ///
    /// Each operation (send, relay, deliver) gets its own span ID.
    pub span_id: Uuid,

    /// Parent span ID - for hierarchical tracing
    ///
    /// Links this span to its parent, enabling visualization of
    /// the call/forward chain.
    pub parent_span_id: Option<Uuid>,

    /// Packet ID as string (for correlation with packet logs)
    pub packet_id: Option<String>,

    /// Original message ID (if this packet is part of a larger flow)
    pub message_id: Option<Uuid>,

    /// Hop count at this point in the trace
    pub hop_count: u32,
}

impl CorrelationContext {
    /// Create a new root context (at message origin)
    ///
    /// Use this when creating a new message that will be sent through the network.
    pub fn new_root() -> Self {
        Self {
            trace_id: Uuid::new_v4(),
            span_id: Uuid::new_v4(),
            parent_span_id: None,
            packet_id: None,
            message_id: Some(Uuid::new_v4()),
            hop_count: 0,
        }
    }

    /// Create a root context with a specific message ID
    pub fn with_message_id(message_id: Uuid) -> Self {
        Self {
            trace_id: Uuid::new_v4(),
            span_id: Uuid::new_v4(),
            parent_span_id: None,
            packet_id: None,
            message_id: Some(message_id),
            hop_count: 0,
        }
    }

    /// Create a child context (for relay/propagation)
    ///
    /// Use this when forwarding a packet to create a new span
    /// that's linked to the parent.
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id: Uuid::new_v4(),
            parent_span_id: Some(self.span_id),
            packet_id: self.packet_id.clone(),
            message_id: self.message_id,
            hop_count: self.hop_count + 1,
        }
    }

    /// Attach a packet ID to this context
    pub fn with_packet_id(mut self, packet_id: impl Into<String>) -> Self {
        self.packet_id = Some(packet_id.into());
        self
    }

    /// Get the trace ID as a string (for logging)
    pub fn trace_id_str(&self) -> String {
        self.trace_id.to_string()
    }

    /// Get the span ID as a string (for logging)
    pub fn span_id_str(&self) -> String {
        self.span_id.to_string()
    }

    /// Get the parent span ID as a string (for logging)
    pub fn parent_span_id_str(&self) -> Option<String> {
        self.parent_span_id.map(|id| id.to_string())
    }

    /// Convert to W3C trace context format (traceparent header)
    ///
    /// Format: `00-{trace_id}-{span_id}-{flags}`
    pub fn to_traceparent(&self) -> String {
        // Convert UUIDs to the 32-char hex format expected by W3C
        let trace_id_hex = self.trace_id.as_simple().to_string();
        let span_id_hex = &self.span_id.as_simple().to_string()[..16]; // Take first 16 chars (8 bytes)
        format!("00-{}-{}-01", trace_id_hex, span_id_hex)
    }

    /// Parse from W3C trace context format
    pub fn from_traceparent(traceparent: &str) -> Option<Self> {
        let parts: Vec<&str> = traceparent.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        let trace_id = Uuid::parse_str(parts[1]).ok()?;
        // Span ID in W3C format is 16 hex chars, we need to pad to 32 for UUID
        let span_id_padded = format!("{}0000000000000000", parts[2]);
        let span_id = Uuid::parse_str(&span_id_padded).ok()?;

        Some(Self {
            trace_id,
            span_id,
            parent_span_id: None,
            packet_id: None,
            message_id: None,
            hop_count: 0,
        })
    }
}

impl Default for CorrelationContext {
    fn default() -> Self {
        Self::new_root()
    }
}

/// Helper trait to attach correlation context to tracing spans
pub trait CorrelationExt {
    /// Record correlation fields on a span
    fn record_correlation(&self, ctx: &CorrelationContext);
}

impl CorrelationExt for tracing::Span {
    fn record_correlation(&self, ctx: &CorrelationContext) {
        self.record("trace_id", ctx.trace_id_str());
        self.record("span_id", ctx.span_id_str());
        if let Some(parent) = ctx.parent_span_id_str() {
            self.record("parent_span_id", parent);
        }
        if let Some(ref packet_id) = ctx.packet_id {
            self.record("packet_id", packet_id.as_str());
        }
        if let Some(message_id) = ctx.message_id {
            self.record("message_id", message_id.to_string());
        }
        self.record("hop_count", ctx.hop_count);
    }
}

/// Standard field names for correlation
pub mod fields {
    pub const TRACE_ID: &str = "trace_id";
    pub const SPAN_ID: &str = "span_id";
    pub const PARENT_SPAN_ID: &str = "parent_span_id";
    pub const PACKET_ID: &str = "packet_id";
    pub const MESSAGE_ID: &str = "message_id";
    pub const HOP_COUNT: &str = "hop_count";
    pub const PEER_ID: &str = "peer_id";
    pub const SOURCE: &str = "source";
    pub const DESTINATION: &str = "destination";
    pub const DECISION: &str = "decision";
    pub const REASON: &str = "reason";
    pub const TTL: &str = "ttl";
    pub const LATENCY_MS: &str = "latency_ms";
}

/// Standard span names for consistency across crates
pub mod spans {
    // Routing spans
    pub const ROUTE_PACKET: &str = "route_packet";
    pub const RELAY_PACKET: &str = "relay_packet";
    pub const DELIVER_PACKET: &str = "deliver_packet";
    pub const BACKPROP: &str = "backprop";
    pub const BACKPROP_STEP: &str = "backprop_step";

    // Transport spans
    pub const CONNECT: &str = "connect";
    pub const ACCEPT: &str = "accept";
    pub const SEND_WIRE_MESSAGE: &str = "send_wire_message";
    pub const RECEIVE_WIRE_MESSAGE: &str = "receive_wire_message";
    pub const PEER_DISCOVERED: &str = "peer_discovered";
    pub const PEER_LOST: &str = "peer_lost";

    // Sync spans
    pub const SYNC_DOCUMENT: &str = "sync_document";
    pub const MERGE_CHANGES: &str = "merge_changes";
    pub const EVENT_PROPAGATE: &str = "event_propagate";

    // DTN spans
    pub const BUNDLE_CREATE: &str = "bundle_create";
    pub const CUSTODY_TRANSFER: &str = "custody_transfer";
    pub const EPIDEMIC_SPRAY: &str = "epidemic_spray";
    pub const BUNDLE_EXPIRE: &str = "bundle_expire";

    // Gossip spans
    pub const GOSSIP_PUBLISH: &str = "gossip_publish";
    pub const GOSSIP_RECEIVE: &str = "gossip_receive";
    pub const GOSSIP_JOIN_TOPIC: &str = "gossip_join_topic";

    // Messaging spans
    pub const SEND_MESSAGE: &str = "send_message";
    pub const RECEIVE_MESSAGE: &str = "receive_message";
    pub const INTERFACE_CREATE: &str = "interface_create";
    pub const INTERFACE_JOIN: &str = "interface_join";

    // SyncEngine Operations
    pub const SYNC_ENGINE_NETWORK_CREATE: &str = "sync_engine_network_create";
    pub const SYNC_ENGINE_NETWORK_START: &str = "sync_engine_network_start";
    pub const SYNC_ENGINE_NETWORK_STOP: &str = "sync_engine_network_stop";
    pub const SYNC_ENGINE_REALM_CREATE: &str = "sync_engine_realm_create";
    pub const SYNC_ENGINE_REALM_JOIN: &str = "sync_engine_realm_join";
    pub const SYNC_ENGINE_MESSAGE_SEND: &str = "sync_engine_message_send";
    pub const SYNC_ENGINE_MESSAGE_RECEIVE: &str = "sync_engine_message_receive";
    pub const SYNC_ENGINE_DOCUMENT_CREATE: &str = "sync_engine_document_create";
    pub const SYNC_ENGINE_DOCUMENT_UPDATE: &str = "sync_engine_document_update";
    pub const SYNC_ENGINE_DOCUMENT_SYNC: &str = "sync_engine_document_sync";
    pub const SYNC_ENGINE_ARTIFACT_SHARE: &str = "sync_engine_artifact_share";
    pub const SYNC_ENGINE_ARTIFACT_DOWNLOAD: &str = "sync_engine_artifact_download";
    pub const SYNC_ENGINE_MEMBER_JOIN: &str = "sync_engine_member_join";
    pub const SYNC_ENGINE_MEMBER_LEAVE: &str = "sync_engine_member_leave";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_root_context() {
        let ctx = CorrelationContext::new_root();
        assert!(ctx.parent_span_id.is_none());
        assert!(ctx.message_id.is_some());
        assert_eq!(ctx.hop_count, 0);
    }

    #[test]
    fn test_child_context() {
        let root = CorrelationContext::new_root();
        let child = root.child();

        // Same trace ID
        assert_eq!(child.trace_id, root.trace_id);
        // Different span ID
        assert_ne!(child.span_id, root.span_id);
        // Parent links to root
        assert_eq!(child.parent_span_id, Some(root.span_id));
        // Hop count incremented
        assert_eq!(child.hop_count, 1);
    }

    #[test]
    fn test_chain_of_children() {
        let root = CorrelationContext::new_root();
        let child1 = root.child();
        let child2 = child1.child();
        let child3 = child2.child();

        // All share same trace ID
        assert_eq!(child1.trace_id, root.trace_id);
        assert_eq!(child2.trace_id, root.trace_id);
        assert_eq!(child3.trace_id, root.trace_id);

        // Hop counts increment
        assert_eq!(child1.hop_count, 1);
        assert_eq!(child2.hop_count, 2);
        assert_eq!(child3.hop_count, 3);
    }

    #[test]
    fn test_with_packet_id() {
        let ctx = CorrelationContext::new_root().with_packet_id("0041#3");
        assert_eq!(ctx.packet_id, Some("0041#3".to_string()));
    }

    #[test]
    fn test_traceparent_roundtrip() {
        let ctx = CorrelationContext::new_root();
        let traceparent = ctx.to_traceparent();

        // Should start with version "00"
        assert!(traceparent.starts_with("00-"));

        // Should be parseable (though we lose some info)
        let parsed = CorrelationContext::from_traceparent(&traceparent);
        assert!(parsed.is_some());

        let parsed = parsed.unwrap();
        assert_eq!(parsed.trace_id, ctx.trace_id);
    }
}
