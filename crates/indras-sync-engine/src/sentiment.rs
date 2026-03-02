//! Sentiment relay - second-degree trust signal propagation.
//!
//! These types are now defined in `indras-network::sentiment` and re-exported
//! here for backward compatibility.

pub use indras_network::sentiment::{
    RelayedSentiment, SentimentRelayDocument, SentimentView, DEFAULT_RELAY_ATTENUATION,
};
