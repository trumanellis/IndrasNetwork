//! # Indras Gossip
//!
//! Topic-based pub/sub gossip protocol for Indras Network, built on iroh-gossip.
//!
//! This crate provides a high-level interface for gossip-based message broadcasting
//! within N-peer interfaces. Messages are signed to ensure authenticity.
//!
//! ## Features
//!
//! - Topic-based message dissemination (one topic per interface)
//! - Automatic message signing and verification
//! - Split sender/receiver handles for concurrent access
//! - Integration with iroh endpoints and routers
//!
//! ## Example
//!
//! ```rust,ignore
//! use indras_gossip::{IndrasGossip, IndrasGossipBuilder};
//! use indras_core::{InterfaceId, InterfaceEvent, SimulationIdentity};
//! use iroh::protocol::Router;
//!
//! // Create endpoint and gossip node
//! let endpoint = iroh::Endpoint::builder().bind().await?;
//! let secret_key = iroh::SecretKey::generate(&mut rand::rng());
//!
//! let gossip: IndrasGossip<SimulationIdentity> = IndrasGossipBuilder::new()
//!     .secret_key(secret_key)
//!     .build(&endpoint);
//!
//! // Register with router
//! let router = Router::builder(endpoint.clone())
//!     .accept(IndrasGossip::<SimulationIdentity>::alpn(), gossip.gossip().clone())
//!     .spawn();
//!
//! // Subscribe to an interface
//! let interface_id = InterfaceId::generate();
//! let split = gossip.subscribe(interface_id, vec![]).await?;
//!
//! // Send messages
//! let peer = SimulationIdentity::new('A').unwrap();
//! let event = InterfaceEvent::message(peer, 1, b"Hello".to_vec());
//! split.sender.broadcast(&event).await?;
//!
//! // Receive messages in another task
//! tokio::spawn(async move {
//!     let mut receiver = split.receiver;
//!     while let Some(result) = receiver.recv().await {
//!         match result {
//!             Ok(event) => println!("Received: {:?}", event),
//!             Err(e) => eprintln!("Error: {}", e),
//!         }
//!     }
//! });
//! ```

pub mod error;
pub mod events;
pub mod message;
pub mod node;
pub mod topic;

// Re-exports
pub use error::{GossipError, GossipResult};
pub use events::{GossipNodeEvent, SimpleGossipEvent};
pub use message::{ReceivedMessage, SignedMessage, WireMessage};
pub use node::{IndrasGossip, IndrasGossipBuilder};
pub use topic::{SplitTopic, TopicHandle, TopicReceiver};

// Re-export iroh-gossip ALPN for router registration
pub use iroh_gossip::net::GOSSIP_ALPN;
