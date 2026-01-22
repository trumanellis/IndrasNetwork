//! # Indras Messaging
//!
//! Encrypted messaging protocol for Indras Network.
//!
//! High-level API for sending and receiving encrypted messages
//! with delivery confirmations and message history.
//!
//! ## Features
//!
//! - End-to-end encrypted messages within interfaces
//! - Gossip-based message delivery
//! - Message history and querying
//! - Reply threading
//! - Multiple content types (text, binary, files)
//!
//! ## Example
//!
//! ```rust,ignore
//! use indras_messaging::{MessagingClient, MessageContent};
//! use indras_gossip::{IndrasGossip, IndrasGossipBuilder};
//! use indras_core::SimulationIdentity;
//!
//! // Create endpoint and gossip node
//! let endpoint = iroh::Endpoint::builder().bind().await?;
//! let secret_key = iroh::SecretKey::generate(&mut rand::rng());
//! let gossip: IndrasGossip<SimulationIdentity> = IndrasGossipBuilder::new()
//!     .secret_key(secret_key)
//!     .build(&endpoint);
//!
//! // Create messaging client
//! let identity = SimulationIdentity::new('A').unwrap();
//! let client = MessagingClient::new(identity, Arc::new(gossip));
//!
//! // Create an interface
//! let (interface_id, invite) = client.create_interface().await?;
//!
//! // Send a message
//! client.send_text(&interface_id, "Hello, world!").await?;
//!
//! // Receive messages
//! let mut rx = client.messages();
//! while let Ok(msg) = rx.recv().await {
//!     println!("{}: {:?}", msg.sender, msg.content);
//! }
//! ```

pub mod client;
pub mod error;
pub mod history;
pub mod message;

// Re-exports
pub use client::MessagingClient;
pub use error::{MessagingError, MessagingResult};
pub use history::{MessageFilter, MessageHistory};
pub use message::{EncryptionMetadata, Message, MessageContent, MessageEnvelope, MessageId};
