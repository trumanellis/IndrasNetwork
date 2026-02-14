//! Outbound forwarder: bridges sync CollabPlugin calls to async IndrasNode
//!
//! `receive_local_update()` is called synchronously by AppFlowy's Collab.
//! We push updates into an mpsc channel; a background tokio task drains the
//! channel and calls `node.send_message()`.

use std::sync::Arc;

use indras_core::InterfaceId;
use indras_node::IndrasNode;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::envelope::AppFlowyEnvelope;
use crate::error::BridgeError;

/// A message queued for outbound delivery.
pub(crate) struct OutboundMessage {
    pub interface_id: InterfaceId,
    pub envelope_bytes: Vec<u8>,
}

/// Outbound forwarder that bridges sync -> async via an mpsc channel.
pub struct OutboundForwarder {
    tx: mpsc::UnboundedSender<OutboundMessage>,
}

impl OutboundForwarder {
    /// Create a new outbound forwarder and spawn the background drain task.
    ///
    /// The returned handle can be used from synchronous code (e.g. `receive_local_update`).
    /// The background task runs until the sender is dropped.
    pub fn spawn(node: Arc<IndrasNode>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(Self::drain_loop(node, rx));
        Self { tx }
    }

    /// Queue an update for outbound delivery.
    ///
    /// This is safe to call from synchronous code — it never blocks.
    pub fn send(
        &self,
        interface_id: InterfaceId,
        object_id_hash: [u8; 32],
        update: Vec<u8>,
    ) -> Result<(), BridgeError> {
        let envelope = AppFlowyEnvelope::new(object_id_hash, update);
        let envelope_bytes = envelope.to_bytes()?;

        self.tx
            .send(OutboundMessage {
                interface_id,
                envelope_bytes,
            })
            .map_err(|_| BridgeError::ChannelClosed)
    }

    /// Background task that drains the channel and sends via IndrasNode.
    async fn drain_loop(
        node: Arc<IndrasNode>,
        mut rx: mpsc::UnboundedReceiver<OutboundMessage>,
    ) {
        while let Some(msg) = rx.recv().await {
            match node.send_message(&msg.interface_id, msg.envelope_bytes).await {
                Ok(event_id) => {
                    debug!(?event_id, "outbound update sent");
                }
                Err(e) => {
                    warn!(error = %e, "failed to send outbound update");
                }
            }
        }
        debug!("outbound forwarder stopped (channel closed)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outbound_message_construction() {
        let object_hash = [0xABu8; 32];
        let update = vec![1, 2, 3];
        let envelope = AppFlowyEnvelope::new(object_hash, update.clone());
        let bytes = envelope.to_bytes().unwrap();
        assert!(!bytes.is_empty());
    }
}
