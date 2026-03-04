//! Attention tip tracking for chain sync protocol.
//!
//! Each peer advertises their latest (author, seq, hash) tip for every
//! attention chain they track. Tips are exchanged via `Document<T>` merge
//! to discover gaps, then direct chain sync fills them in.
//!
//! # CRDT Semantics
//!
//! - Merge strategy: per-author max-seq wins
//! - Concurrent tips from different peers: keep highest seq per author

use indras_network::member::MemberId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A tip advertisement: the latest event in an author's attention chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttentionTip {
    /// The chain author.
    pub author: MemberId,
    /// Latest sequence number.
    pub seq: u64,
    /// BLAKE3 hash of the latest event.
    pub event_hash: [u8; 32],
    /// Wall-clock time of the tip (milliseconds).
    pub wall_time_ms: i64,
}

/// CRDT document tracking attention chain tips for all known authors.
///
/// Used to discover which peers have new events we haven't seen yet.
/// After merging tips, a peer can request missing events by seq range.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AttentionTipDocument {
    /// Latest tip per author (author MemberId -> tip).
    tips: HashMap<MemberId, AttentionTip>,
}

impl AttentionTipDocument {
    /// Create a new empty tip document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the tip for an author (only if seq is higher than existing).
    pub fn update_tip(&mut self, tip: AttentionTip) {
        let dominated = self
            .tips
            .get(&tip.author)
            .map_or(true, |existing| tip.seq > existing.seq);

        if dominated {
            self.tips.insert(tip.author, tip);
        }
    }

    /// Get the tip for a specific author.
    pub fn tip_for(&self, author: &MemberId) -> Option<&AttentionTip> {
        self.tips.get(author)
    }

    /// Get all tips.
    pub fn all_tips(&self) -> &HashMap<MemberId, AttentionTip> {
        &self.tips
    }

    /// Compare our tips with a peer's to find authors where they're ahead.
    ///
    /// Returns a list of (author, our_seq, their_seq) for chains where
    /// the peer has a higher seq than us (i.e., we need to sync).
    pub fn gaps_from(&self, peer_tips: &AttentionTipDocument) -> Vec<(MemberId, u64, u64)> {
        let mut gaps = Vec::new();
        for (author, peer_tip) in &peer_tips.tips {
            let our_seq = self.tips.get(author).map_or(0, |t| t.seq);
            if peer_tip.seq > our_seq {
                gaps.push((*author, our_seq, peer_tip.seq));
            }
        }
        gaps
    }

    /// Merge another tip document into this one (per-author max-seq wins).
    pub fn merge(&mut self, other: Self) {
        for (author, tip) in other.tips {
            self.update_tip(AttentionTip {
                author,
                ..tip
            });
        }
    }
}
