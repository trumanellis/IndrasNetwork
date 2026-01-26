//! # Indras Transport
//!
//! Transport layer for Indras Network using iroh.
//!
//! Provides peer-to-peer connectivity, connection management,
//! and peer discovery.
//!
//! ## Features
//!
//! - iroh-based QUIC connections with hole punching
//! - Connection pooling and lifecycle management
//! - Peer discovery via iroh-gossip
//! - Wire protocol framing with postcard serialization
//!
//! ## Example
//!
//! ```rust,ignore
//! use indras_transport::{ConnectionManager, ConnectionConfig, IrohIdentity};
//! use iroh::SecretKey;
//!
//! // Create a connection manager
//! let secret = SecretKey::generate(&mut rand::thread_rng());
//! let config = ConnectionConfig::default();
//! let manager = ConnectionManager::new(secret, config).await?;
//!
//! // Get our node address to share with peers
//! let addr = manager.node_addr().await;
//! println!("My address: {:?}", addr);
//!
//! // Connect to another peer
//! let peer_addr = /* get peer's NodeAddr */;
//! let conn = manager.connect(peer_addr).await?;
//! ```

pub mod adapter;
pub mod connection;
pub mod discovery;
pub mod error;
pub mod identity;
pub mod protocol;

// Re-export main types
pub use adapter::{AdapterConfig, AdapterError, IrohNetworkAdapter};
pub use connection::{ConnectionConfig, ConnectionError, ConnectionManager, ConnectionStats};
pub use discovery::{
    DiscoveryConfig, DiscoveryError, DiscoveryService, DiscoveryStats, PeerEvent, PeerInfo,
};
pub use error::TransportError;
pub use identity::IrohIdentity;
pub use protocol::{
    ALPN_INDRAS, InterfaceJoinMessage, InterfaceLeaveMessage, IntroductionRequestMessage,
    IntroductionResponseMessage, PeerIntroductionMessage, PresenceInfo, RealmPeerInfo,
    SerializedConfirmation, SerializedPacket, SyncRequest, SyncResponse, WireMessage,
    frame_message, parse_framed_message,
};

// Re-export iroh types that users will need
pub use iroh::endpoint::Connection;
pub use iroh::{Endpoint, EndpointAddr, PublicKey, SecretKey};
