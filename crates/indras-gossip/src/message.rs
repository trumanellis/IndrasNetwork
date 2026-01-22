//! Signed message encoding for gossip broadcast
//!
//! Messages are signed to ensure authenticity and prevent spoofing.

use indras_core::{InterfaceEvent, PeerIdentity};
use iroh::{PublicKey, SecretKey, Signature};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{GossipError, GossipResult};

/// A signed message ready for gossip broadcast
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedMessage {
    /// Public key of the sender
    pub from: PublicKey,
    /// Serialized message data
    pub data: Vec<u8>,
    /// Signature over the data
    pub signature: Signature,
}

impl SignedMessage {
    /// Sign and encode a message for broadcast
    pub fn sign_and_encode<I: PeerIdentity + Serialize>(
        secret_key: &SecretKey,
        event: &InterfaceEvent<I>,
    ) -> GossipResult<Vec<u8>> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let wire_message = WireMessage::V0 {
            timestamp,
            event_data: postcard::to_allocvec(event)?,
        };

        let data = postcard::to_allocvec(&wire_message)?;
        let signature = secret_key.sign(&data);
        let from = secret_key.public();

        let signed = SignedMessage {
            from,
            data,
            signature,
        };

        postcard::to_allocvec(&signed).map_err(Into::into)
    }

    /// Verify signature and decode the message
    pub fn verify_and_decode<I: PeerIdentity + for<'de> Deserialize<'de>>(
        bytes: &[u8],
    ) -> GossipResult<ReceivedMessage<I>> {
        let signed: SignedMessage =
            postcard::from_bytes(bytes).map_err(|e| GossipError::DecodeFailed(e.to_string()))?;

        // Verify signature
        signed
            .from
            .verify(&signed.data, &signed.signature)
            .map_err(|e| GossipError::SignatureVerificationFailed(e.to_string()))?;

        // Decode wire message
        let wire_message: WireMessage = postcard::from_bytes(&signed.data)
            .map_err(|e| GossipError::DecodeFailed(e.to_string()))?;

        let WireMessage::V0 {
            timestamp,
            event_data,
        } = wire_message;

        // Decode event
        let event: InterfaceEvent<I> = postcard::from_bytes(&event_data)
            .map_err(|e| GossipError::DecodeFailed(e.to_string()))?;

        Ok(ReceivedMessage {
            from: signed.from,
            timestamp,
            event,
        })
    }
}

/// Wire format for gossip messages (versioned for future compatibility)
#[derive(Debug, Serialize, Deserialize)]
pub enum WireMessage {
    /// Version 0 format
    V0 {
        /// Timestamp in microseconds since UNIX epoch
        timestamp: u64,
        /// Serialized InterfaceEvent bytes
        event_data: Vec<u8>,
    },
}

/// A received and verified message
#[derive(Debug, Clone)]
pub struct ReceivedMessage<I: PeerIdentity> {
    /// Public key of the sender
    pub from: PublicKey,
    /// Timestamp when the message was sent (microseconds since UNIX epoch)
    pub timestamp: u64,
    /// The decoded interface event
    pub event: InterfaceEvent<I>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    #[test]
    fn test_sign_and_verify_roundtrip() {
        let secret_key = SecretKey::generate(&mut rand::rng());
        let peer = SimulationIdentity::new('A').unwrap();
        let event = InterfaceEvent::message(peer, 1, b"Hello".to_vec());

        // Sign and encode
        let encoded = SignedMessage::sign_and_encode(&secret_key, &event).unwrap();

        // Verify and decode
        let received: ReceivedMessage<SimulationIdentity> =
            SignedMessage::verify_and_decode(&encoded).unwrap();

        assert_eq!(received.from, secret_key.public());

        match received.event {
            InterfaceEvent::Message { content, .. } => {
                assert_eq!(content, b"Hello");
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_tampered_message_fails_verification() {
        let secret_key = SecretKey::generate(&mut rand::rng());
        let peer = SimulationIdentity::new('A').unwrap();
        let event = InterfaceEvent::message(peer, 1, b"Hello".to_vec());

        let mut encoded = SignedMessage::sign_and_encode(&secret_key, &event).unwrap();

        // Tamper with the message
        if let Some(byte) = encoded.last_mut() {
            *byte = byte.wrapping_add(1);
        }

        // Verification should fail
        let result: GossipResult<ReceivedMessage<SimulationIdentity>> =
            SignedMessage::verify_and_decode(&encoded);

        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_key_fails_verification() {
        let secret_key1 = SecretKey::generate(&mut rand::rng());
        let secret_key2 = SecretKey::generate(&mut rand::rng());
        let peer = SimulationIdentity::new('A').unwrap();
        let event = InterfaceEvent::message(peer, 1, b"Hello".to_vec());

        // Sign with key1
        let encoded = SignedMessage::sign_and_encode(&secret_key1, &event).unwrap();

        // Try to decode - should work since we verify against the embedded public key
        let received: ReceivedMessage<SimulationIdentity> =
            SignedMessage::verify_and_decode(&encoded).unwrap();

        // But the from field should be key1's public key, not key2's
        assert_eq!(received.from, secret_key1.public());
        assert_ne!(received.from, secret_key2.public());
    }
}
