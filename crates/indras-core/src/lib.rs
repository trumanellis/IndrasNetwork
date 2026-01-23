//! # Indras Core
//!
//! Core traits, types, and errors for the Indras Network stack.
//!
//! This crate provides the foundational abstractions that allow the same
//! routing and messaging logic to work with both simulation (for testing)
//! and real networking (using iroh).
//!
//! ## Key Traits
//!
//! - [`PeerIdentity`]: Abstraction over peer identification (char for sim, PublicKey for real)
//! - [`NetworkTopology`]: Abstraction over network structure
//! - [`NInterfaceTrait`]: N-peer shared interface with append-only event log
//! - [`Clock`]: Time abstraction for testability
//!
//! ## Key Types
//!
//! - [`InterfaceId`]: Unique identifier for an N-peer interface
//! - [`InterfaceEvent`]: Events in an interface (messages, membership, presence)
//! - [`Packet`]: A sealed packet for store-and-forward delivery
//! - [`NetworkEvent`]: Events that occur in the network

pub mod error;
pub mod identity;
pub mod packet;
pub mod event;
pub mod routing;
pub mod traits;
pub mod interface;
pub mod transport;
pub mod mock_transport;

// Re-export main types
pub use error::*;
pub use identity::*;
pub use packet::*;
pub use event::*;
pub use routing::*;
pub use traits::*;
pub use interface::*;
pub use transport::*;
pub use mock_transport::*;
