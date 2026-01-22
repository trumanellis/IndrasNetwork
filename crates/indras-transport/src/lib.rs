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

pub mod identity;
pub mod protocol;
pub mod connection;
pub mod discovery;
pub mod error;

// Re-export main types
pub use identity::IrohIdentity;
pub use protocol::{
    ALPN_INDRAS,
    WireMessage,
    SerializedPacket,
    SerializedConfirmation,
    PresenceInfo,
    SyncRequest,
    SyncResponse,
    frame_message,
    parse_framed_message,
};
pub use connection::{
    ConnectionManager,
    ConnectionConfig,
    ConnectionError,
    ConnectionStats,
};
pub use discovery::{
    DiscoveryService,
    DiscoveryConfig,
    DiscoveryError,
    PeerEvent,
    PeerInfo,
    DiscoveryStats,
};
pub use error::TransportError;

// Re-export iroh types that users will need
pub use iroh::{SecretKey, PublicKey, EndpointAddr, Endpoint};
pub use iroh::endpoint::Connection;
