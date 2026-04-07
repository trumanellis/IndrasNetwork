//! Relay client for connecting to blind store-and-forward relay servers
//!
//! Provides `RelayClient` for establishing connections and `RelaySession`
//! for authenticated relay protocol operations (register, store, retrieve).
//!
//! ## Example
//!
//! ```rust,ignore
//! use indras_transport::relay_client::RelayClient;
//! use ed25519_dalek::SigningKey;
//! use iroh::SecretKey;
//!
//! let client = RelayClient::new(signing_key, transport_secret);
//! let mut session = client.connect(relay_addr).await?;
//! let auth_ack = session.authenticate().await?;
//! ```

use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointAddr, SecretKey};
use tracing::{debug, warn};

use indras_core::{EventId, InterfaceId};
use indras_crypto::credential;

use crate::error::TransportError;
use crate::protocol::{
    frame_message, RelayAuthMessage, RelayContactsSyncMessage, RelayContactsSyncAckMessage,
    RelayDeliveryMessage, RelayRegisterAckMessage, RelayRegisterMessage, RelayRetrieveMessage,
    RelayStoreAckMessage, RelayStoreMessage, RelayUnregisterMessage, StoreContentType,
    StoreMetadata, StorageTier, WireMessage, ALPN_INDRAS, MAX_MESSAGE_SIZE,
};

/// Client for connecting to an Indras relay server
#[derive(Clone)]
pub struct RelayClient {
    /// Ed25519 signing key for credential creation
    signing_key: SigningKey,
    /// Transport secret key for QUIC connections
    transport_secret: SecretKey,
    /// Credential validity duration
    credential_ttl: Duration,
}

impl RelayClient {
    /// Create a new relay client
    ///
    /// - `signing_key`: Ed25519 key whose public half is the player ID
    /// - `transport_secret`: iroh secret key for QUIC transport
    pub fn new(signing_key: SigningKey, transport_secret: SecretKey) -> Self {
        Self {
            signing_key,
            transport_secret,
            credential_ttl: Duration::from_secs(3600),
        }
    }

    /// Set the credential TTL (how long credentials are valid)
    pub fn with_credential_ttl(mut self, ttl: Duration) -> Self {
        self.credential_ttl = ttl;
        self
    }

    /// Create a reusable QUIC endpoint for connecting to multiple relays.
    ///
    /// Use with `connect_with_endpoint` to share one endpoint across
    /// many relay sessions instead of creating one endpoint per session.
    pub async fn create_endpoint(&self) -> Result<Endpoint, TransportError> {
        Endpoint::builder()
            .secret_key(self.transport_secret.clone())
            .alpns(vec![ALPN_INDRAS.to_vec()])
            .bind()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Failed to create endpoint: {e}")))
    }

    /// Connect to a relay server using a shared endpoint.
    pub async fn connect_with_endpoint(
        &self,
        endpoint: &Endpoint,
        relay_addr: EndpointAddr,
    ) -> Result<RelaySession, TransportError> {
        let connection: Connection = endpoint
            .connect(relay_addr, ALPN_INDRAS)
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Failed to connect to relay: {e}")))?;

        let (send_stream, recv_stream) = connection
            .open_bi()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Failed to open stream: {e}")))?;

        let transport_pubkey = *self.transport_secret.public().as_bytes();

        debug!("Connected to relay server (shared endpoint)");

        Ok(RelaySession {
            signing_key: self.signing_key.clone(),
            transport_pubkey,
            credential_ttl: self.credential_ttl,
            send_stream,
            recv_stream,
            _endpoint: endpoint.clone(),
        })
    }

    /// Connect to a relay server and return an active session
    ///
    /// Creates a QUIC endpoint, connects to the relay, and opens a
    /// bidirectional stream for the relay protocol exchange.
    pub async fn connect(&self, relay_addr: EndpointAddr) -> Result<RelaySession, TransportError> {
        let endpoint = self.create_endpoint().await?;

        let connection: Connection = endpoint
            .connect(relay_addr, ALPN_INDRAS)
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Failed to connect to relay: {e}")))?;

        let (send_stream, recv_stream) = connection
            .open_bi()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Failed to open stream: {e}")))?;

        let transport_pubkey = *self.transport_secret.public().as_bytes();

        debug!("Connected to relay server");

        Ok(RelaySession {
            signing_key: self.signing_key.clone(),
            transport_pubkey,
            credential_ttl: self.credential_ttl,
            send_stream,
            recv_stream,
            _endpoint: endpoint,
        })
    }
}

/// An active session with a relay server
///
/// Wraps a bidirectional QUIC stream and provides typed methods
/// for each relay protocol operation.
pub struct RelaySession {
    signing_key: SigningKey,
    transport_pubkey: [u8; 32],
    credential_ttl: Duration,
    send_stream: iroh::endpoint::SendStream,
    recv_stream: iroh::endpoint::RecvStream,
    /// Keep the endpoint alive for the duration of the session
    _endpoint: Endpoint,
}

impl RelaySession {
    /// Authenticate with the relay using a signed credential
    ///
    /// Creates a fresh credential linking the player ID to the transport
    /// key, signs it, and sends it to the relay.
    pub async fn authenticate(
        &mut self,
    ) -> Result<crate::protocol::RelayAuthAckMessage, TransportError> {
        let player_id = self.signing_key.verifying_key().to_bytes();

        let expires =
            chrono::Utc::now().timestamp_millis() + self.credential_ttl.as_millis() as i64;
        let cred_bytes =
            credential::create_credential(&self.signing_key, self.transport_pubkey, expires);

        let msg = WireMessage::RelayAuth(RelayAuthMessage {
            credential: cred_bytes,
            player_id,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        });

        self.send_message(&msg).await?;
        let response = self.recv_message().await?;

        match response {
            WireMessage::RelayAuthAck(ack) => {
                if ack.authenticated {
                    debug!(tiers = ?ack.granted_tiers, "Authenticated with relay");
                } else {
                    warn!("Relay rejected authentication");
                }
                Ok(ack)
            }
            other => Err(TransportError::Protocol(format!(
                "Expected RelayAuthAck, got {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    /// Register interfaces for store-and-forward
    pub async fn register(
        &mut self,
        interfaces: Vec<InterfaceId>,
    ) -> Result<RelayRegisterAckMessage, TransportError> {
        let msg = WireMessage::RelayRegister(RelayRegisterMessage::new(interfaces));
        self.send_message(&msg).await?;
        let response = self.recv_message().await?;

        match response {
            WireMessage::RelayRegisterAck(ack) => Ok(ack),
            other => Err(TransportError::Protocol(format!(
                "Expected RelayRegisterAck, got {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    /// Unregister interfaces from the relay
    pub async fn unregister(&mut self, interfaces: Vec<InterfaceId>) -> Result<(), TransportError> {
        let msg = WireMessage::RelayUnregister(RelayUnregisterMessage::new(interfaces));
        self.send_message(&msg).await?;
        Ok(())
    }

    /// Store data in a specific tier
    pub async fn store(
        &mut self,
        tier: StorageTier,
        interface_id: InterfaceId,
        data: Vec<u8>,
        metadata: StoreMetadata,
    ) -> Result<RelayStoreAckMessage, TransportError> {
        let msg = WireMessage::RelayStore(RelayStoreMessage {
            tier,
            interface_id,
            data,
            metadata,
            timestamp_millis: chrono::Utc::now().timestamp_millis(),
        });
        self.send_message(&msg).await?;
        let response = self.recv_message().await?;

        match response {
            WireMessage::RelayStoreAck(ack) => Ok(ack),
            other => Err(TransportError::Protocol(format!(
                "Expected RelayStoreAck, got {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    /// Store a simple event (convenience wrapper)
    pub async fn store_event(
        &mut self,
        tier: StorageTier,
        interface_id: InterfaceId,
        data: Vec<u8>,
    ) -> Result<RelayStoreAckMessage, TransportError> {
        self.store(
            tier,
            interface_id,
            data,
            StoreMetadata {
                content_type: StoreContentType::Event,
                pin: false,
                ttl_override_days: None,
            },
        )
        .await
    }

    /// Retrieve stored events for an interface
    pub async fn retrieve(
        &mut self,
        interface_id: InterfaceId,
        after: Option<EventId>,
        tier: Option<StorageTier>,
    ) -> Result<RelayDeliveryMessage, TransportError> {
        let mut msg = RelayRetrieveMessage::new(interface_id);
        msg.after_event_id = after;
        msg.tier = tier;

        self.send_message(&WireMessage::RelayRetrieve(msg)).await?;
        let response = self.recv_message().await?;

        match response {
            WireMessage::RelayDelivery(delivery) => Ok(delivery),
            other => Err(TransportError::Protocol(format!(
                "Expected RelayDelivery, got {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    /// Sync contacts list (owner only)
    pub async fn sync_contacts(
        &mut self,
        contacts: Vec<[u8; 32]>,
    ) -> Result<RelayContactsSyncAckMessage, TransportError> {
        let msg = WireMessage::RelayContactsSync(RelayContactsSyncMessage { contacts });
        self.send_message(&msg).await?;
        let response = self.recv_message().await?;

        match response {
            WireMessage::RelayContactsSyncAck(ack) => Ok(ack),
            other => Err(TransportError::Protocol(format!(
                "Expected RelayContactsSyncAck, got {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    /// Send a ping and measure round-trip time
    pub async fn ping(&mut self) -> Result<Duration, TransportError> {
        let nonce = rand::random::<u64>();
        let start = Instant::now();

        self.send_message(&WireMessage::Ping(nonce)).await?;
        let response = self.recv_message().await?;

        match response {
            WireMessage::Pong(n) if n == nonce => Ok(start.elapsed()),
            WireMessage::Pong(_) => Err(TransportError::Protocol("Ping nonce mismatch".into())),
            other => Err(TransportError::Protocol(format!(
                "Expected Pong, got {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    /// Send a framed wire message
    async fn send_message(&mut self, msg: &WireMessage) -> Result<(), TransportError> {
        let framed = frame_message(msg)
            .map_err(|e| TransportError::Protocol(format!("Failed to frame message: {e}")))?;
        self.send_stream
            .write_all(&framed)
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Failed to send: {e}")))?;
        Ok(())
    }

    /// Receive and parse a framed wire message
    async fn recv_message(&mut self) -> Result<WireMessage, TransportError> {
        // Read 4-byte length prefix
        let mut len_buf = [0u8; 4];
        self.recv_stream
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Failed to read length: {e}")))?;

        let msg_len = u32::from_be_bytes(len_buf) as usize;
        if msg_len > MAX_MESSAGE_SIZE {
            return Err(TransportError::Protocol(format!(
                "Message too large: {msg_len} > {MAX_MESSAGE_SIZE}"
            )));
        }

        // Read message body
        let mut msg_buf = vec![0u8; msg_len];
        self.recv_stream
            .read_exact(&mut msg_buf)
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Failed to read message: {e}")))?;

        let msg: WireMessage = postcard::from_bytes(&msg_buf)
            .map_err(|e| TransportError::Protocol(format!("Failed to deserialize: {e}")))?;

        Ok(msg)
    }
}
