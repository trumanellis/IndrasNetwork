//! Sentiment relay - second-degree trust signal propagation.
//!
//! Each peer publishes a `SentimentRelayDocument` containing their relayable
//! sentiment ratings. Contacts can read this document to get second-degree
//! signals about people they don't directly know.
//!
//! The relay is scoped: you only see sentiment from your own contacts, and
//! relayed sentiment from your contacts' contacts. No global reputation scores.

use indras_network::member::MemberId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A sentiment signal relayed through a contact.
#[derive(Debug, Clone)]
pub struct RelayedSentiment {
    /// The member this sentiment is about.
    pub about: MemberId,
    /// The sentiment value (-1, 0, or +1).
    pub sentiment: i8,
    /// Which of our direct contacts relayed this signal.
    pub relay_source: MemberId,
    /// Degree of separation: 1 = direct contact's opinion, 2 = relayed.
    pub degree: u8,
}

/// Aggregated sentiment view about a specific member.
///
/// Combines direct signals (from our own contacts) with relayed signals
/// (from contacts' contacts), providing a local, Sybil-resistant
/// perspective on another member's reputation.
#[derive(Debug, Clone, Default)]
pub struct SentimentView {
    /// Direct sentiment from our own contacts who know the target.
    pub direct: Vec<(MemberId, i8)>,
    /// Second-degree sentiment relayed through our contacts.
    pub relayed: Vec<RelayedSentiment>,
}

impl SentimentView {
    /// Compute a weighted sentiment score.
    ///
    /// Direct signals have full weight (1.0), relayed signals are attenuated
    /// by the given factor (e.g., 0.3). Returns None if there are no signals.
    pub fn weighted_score(&self, relay_attenuation: f64) -> Option<f64> {
        if self.direct.is_empty() && self.relayed.is_empty() {
            return None;
        }

        let direct_sum: f64 = self.direct.iter().map(|(_, s)| *s as f64).sum();
        let direct_count = self.direct.len() as f64;

        let relayed_sum: f64 = self
            .relayed
            .iter()
            .map(|r| r.sentiment as f64 * relay_attenuation)
            .sum();
        let relayed_count = self.relayed.len() as f64 * relay_attenuation;

        let total_weight = direct_count + relayed_count;
        if total_weight == 0.0 {
            return None;
        }

        Some((direct_sum + relayed_sum) / total_weight)
    }

    /// Count of unique signal sources (both direct and relayed).
    pub fn signal_count(&self) -> usize {
        self.direct.len() + self.relayed.len()
    }

    /// Whether the aggregate sentiment is negative (below threshold).
    pub fn is_negative(&self, threshold: f64, relay_attenuation: f64) -> bool {
        self.weighted_score(relay_attenuation)
            .map(|s| s < threshold)
            .unwrap_or(false)
    }

    /// Whether the aggregate sentiment is positive (above threshold).
    pub fn is_positive(&self, threshold: f64, relay_attenuation: f64) -> bool {
        self.weighted_score(relay_attenuation)
            .map(|s| s > threshold)
            .unwrap_or(false)
    }
}

/// Document published by each peer containing their relayable sentiment ratings.
///
/// This document is synced within the contacts realm. Each peer publishes
/// one, and their contacts can read it to obtain second-degree signals.
/// Only sentiments marked as `relayable` are included.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SentimentRelayDocument {
    /// Map from member ID to sentiment value.
    /// Only contains entries where the original contact has relayable=true.
    pub sentiments: BTreeMap<MemberId, i8>,
}

impl SentimentRelayDocument {
    pub fn new() -> Self {
        Self {
            sentiments: BTreeMap::new(),
        }
    }

    /// Update from a contacts document's relayable sentiments.
    pub fn update_from(&mut self, relayable: &[(MemberId, i8)]) {
        self.sentiments.clear();
        for (id, sentiment) in relayable {
            self.sentiments.insert(*id, *sentiment);
        }
    }

    /// Get sentiment about a specific member, if any.
    pub fn get(&self, member_id: &MemberId) -> Option<i8> {
        self.sentiments.get(member_id).copied()
    }

    /// Iterate over all sentiments in this relay document.
    pub fn iter(&self) -> impl Iterator<Item = (&MemberId, &i8)> {
        self.sentiments.iter()
    }
}

/// Default relay attenuation factor for second-degree signals.
pub const DEFAULT_RELAY_ATTENUATION: f64 = 0.3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentiment_view_empty() {
        let view = SentimentView::default();
        assert_eq!(view.weighted_score(DEFAULT_RELAY_ATTENUATION), None);
        assert_eq!(view.signal_count(), 0);
    }

    #[test]
    fn test_sentiment_view_direct_only() {
        let view = SentimentView {
            direct: vec![([1u8; 32], 1), ([2u8; 32], 1), ([3u8; 32], -1)],
            relayed: vec![],
        };
        // (1 + 1 + -1) / 3 = 0.333...
        let score = view.weighted_score(DEFAULT_RELAY_ATTENUATION).unwrap();
        assert!((score - 0.333).abs() < 0.01);
        assert_eq!(view.signal_count(), 3);
    }

    #[test]
    fn test_sentiment_view_with_relay() {
        let view = SentimentView {
            direct: vec![([1u8; 32], -1)],
            relayed: vec![
                RelayedSentiment {
                    about: [10u8; 32],
                    sentiment: 1,
                    relay_source: [1u8; 32],
                    degree: 2,
                },
                RelayedSentiment {
                    about: [10u8; 32],
                    sentiment: 1,
                    relay_source: [2u8; 32],
                    degree: 2,
                },
            ],
        };
        // direct: -1 (weight 1.0)
        // relayed: 1*0.3 + 1*0.3 = 0.6 (weight 0.6)
        // total: (-1 + 0.6) / (1.0 + 0.6) = -0.4 / 1.6 = -0.25
        let score = view.weighted_score(DEFAULT_RELAY_ATTENUATION).unwrap();
        assert!((score - (-0.25)).abs() < 0.01);
    }

    #[test]
    fn test_sentiment_relay_document() {
        let mut doc = SentimentRelayDocument::new();
        let relayable = vec![([1u8; 32], 1), ([2u8; 32], -1)];
        doc.update_from(&relayable);

        assert_eq!(doc.get(&[1u8; 32]), Some(1));
        assert_eq!(doc.get(&[2u8; 32]), Some(-1));
        assert_eq!(doc.get(&[3u8; 32]), None);
    }

    #[test]
    fn test_is_negative_positive() {
        let view = SentimentView {
            direct: vec![([1u8; 32], -1), ([2u8; 32], -1)],
            relayed: vec![],
        };
        assert!(view.is_negative(0.0, DEFAULT_RELAY_ATTENUATION));
        assert!(!view.is_positive(0.0, DEFAULT_RELAY_ATTENUATION));

        let view = SentimentView {
            direct: vec![([1u8; 32], 1), ([2u8; 32], 1)],
            relayed: vec![],
        };
        assert!(view.is_positive(0.0, DEFAULT_RELAY_ATTENUATION));
        assert!(!view.is_negative(0.0, DEFAULT_RELAY_ATTENUATION));
    }
}
