//! Inbound listener: receives updates from the P2P network and applies them
//! to the local Yrs Doc.
//!
//! Subscribes to `node.events()` for a specific interface, filters for
//! AppFlowy envelopes (by magic bytes), and applies the contained Yrs
//! updates to the shared document.

use std::sync::Arc;

use indras_core::{InterfaceEvent, InterfaceId};
use indras_node::IndrasNode;
use tokio::sync::broadcast;
use tracing::{debug, trace, warn};
use yrs::updates::decoder::Decode;
use yrs::{Doc, Transact, Update};

use crate::envelope::AppFlowyEnvelope;

/// Background task handle for the inbound listener.
pub struct InboundListener {
    cancel: tokio::sync::watch::Sender<bool>,
}

impl InboundListener {
    /// Spawn an inbound listener for the given interface.
    ///
    /// The listener subscribes to `node.events(interface_id)` and applies
    /// incoming Yrs updates to `doc`. It filters messages using the envelope
    /// magic bytes and the expected `object_id_hash`.
    ///
    /// The listener runs until `shutdown()` is called or the event channel closes.
    pub fn spawn(
        node: Arc<IndrasNode>,
        interface_id: InterfaceId,
        doc: Doc,
        object_id_hash: [u8; 32],
    ) -> Result<Self, crate::error::BridgeError> {
        let rx = node
            .events(&interface_id)
            .map_err(|e| crate::error::BridgeError::Node(e.to_string()))?;

        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        tokio::spawn(Self::listen_loop(rx, doc, object_id_hash, cancel_rx));

        Ok(Self { cancel: cancel_tx })
    }

    /// Stop the inbound listener.
    pub fn shutdown(&self) {
        let _ = self.cancel.send(true);
    }

    /// Background loop that receives events and applies updates.
    async fn listen_loop(
        mut rx: broadcast::Receiver<indras_node::ReceivedEvent>,
        doc: Doc,
        object_id_hash: [u8; 32],
        mut cancel: tokio::sync::watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(received) => {
                            Self::handle_event(&doc, &object_id_hash, &received.event);
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(skipped = n, "inbound listener lagged, some updates may be missed");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            debug!("inbound event channel closed");
                            break;
                        }
                    }
                }
                _ = cancel.changed() => {
                    if *cancel.borrow() {
                        debug!("inbound listener cancelled");
                        break;
                    }
                }
            }
        }
    }

    /// Process a single event: decode envelope, verify object hash, apply update.
    fn handle_event<I: indras_core::PeerIdentity>(
        doc: &Doc,
        expected_hash: &[u8; 32],
        event: &InterfaceEvent<I>,
    ) {
        let content = match event {
            InterfaceEvent::Message { content, .. } => content,
            _ => return, // Only process Message events
        };

        // Try to decode as AppFlowy envelope
        let envelope = match AppFlowyEnvelope::from_bytes(content) {
            Some(Ok(env)) => env,
            Some(Err(e)) => {
                warn!(error = %e, "corrupt AppFlowy envelope");
                return;
            }
            None => {
                trace!("ignoring non-AppFlowy message");
                return;
            }
        };

        // Verify this envelope is for our object
        if &envelope.object_id_hash != expected_hash {
            trace!("ignoring envelope for different object");
            return;
        }

        // Apply the Yrs update
        if envelope.update.is_empty() {
            return;
        }

        match Update::decode_v1(&envelope.update) {
            Ok(update) => {
                let mut txn = doc.transact_mut();
                if let Err(e) = txn.apply_update(update) {
                    warn!(error = %e, "failed to apply inbound yrs update");
                }
                debug!("applied inbound yrs update ({} bytes)", envelope.update.len());
            }
            Err(e) => {
                warn!(error = %e, "failed to decode inbound yrs update");
            }
        }
    }
}

impl Drop for InboundListener {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yrs::{GetString, Text};

    #[test]
    fn test_handle_non_appflowy_message() {
        // Verify that non-AppFlowy messages are silently ignored
        let doc = Doc::new();
        let hash = [0xABu8; 32];

        let event = InterfaceEvent::message(
            indras_core::SimulationIdentity::new('A').unwrap(),
            1,
            b"not an envelope".to_vec(),
        );

        // Should not panic
        InboundListener::handle_event(&doc, &hash, &event);
    }

    #[test]
    fn test_handle_valid_appflowy_update() {
        let doc = Doc::new();
        // Create a real Yrs update
        let source_doc = Doc::new();
        let update_bytes = {
            let text = source_doc.get_or_insert_text("test");
            let mut txn = source_doc.transact_mut();
            text.insert(&mut txn, 0, "hello");
            txn.encode_update_v1()
        };

        let object_id = "test-doc";
        let object_hash = AppFlowyEnvelope::hash_object_id(object_id);
        let envelope = AppFlowyEnvelope::new(object_hash, update_bytes);
        let envelope_bytes = envelope.to_bytes().unwrap();

        let event = InterfaceEvent::message(
            indras_core::SimulationIdentity::new('A').unwrap(),
            1,
            envelope_bytes,
        );

        InboundListener::handle_event(&doc, &object_hash, &event);

        // Verify the update was applied
        let text = doc.get_or_insert_text("test");
        let txn = doc.transact();
        assert_eq!(text.get_string(&txn), "hello");
    }

    #[test]
    fn test_handle_wrong_object_hash() {
        let doc = Doc::new();
        let source_doc = Doc::new();
        let update_bytes = {
            let text = source_doc.get_or_insert_text("test");
            let mut txn = source_doc.transact_mut();
            text.insert(&mut txn, 0, "should not appear");
            txn.encode_update_v1()
        };

        let envelope = AppFlowyEnvelope::new([0xAAu8; 32], update_bytes);
        let envelope_bytes = envelope.to_bytes().unwrap();

        let event = InterfaceEvent::message(
            indras_core::SimulationIdentity::new('A').unwrap(),
            1,
            envelope_bytes,
        );

        // Different hash — should be ignored
        let different_hash = [0xBBu8; 32];
        InboundListener::handle_event(&doc, &different_hash, &event);

        let text = doc.get_or_insert_text("test");
        let txn = doc.transact();
        assert_eq!(text.get_string(&txn), "", "update should not have been applied");
    }
}
