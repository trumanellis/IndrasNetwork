//! Chain sync gap-filling for the locally-conservative attention ledger.
//!
//! Compares local chain tips against the `AttentionTipDocument` to detect
//! gaps, then relies on `Document<AttentionDocument>` CRDT sync for
//! gap-filling. After sync, validates received chains for integrity.
//!
//! # Phase 1 Design
//!
//! Gap-filling relies on the existing CRDT merge path (`Document<T>` sync).
//! A dedicated seq-range request protocol can be added in Phase 1.5 if
//! the CRDT approach proves insufficient at scale.

use crate::attention::AttentionDocument;
use crate::attention_tip::AttentionTipDocument;
use indras_artifacts::artifact::ArtifactId;
use indras_artifacts::attention::validate::{validate_chain, AuthorState, ValidationError};
use indras_network::member::MemberId;
use std::collections::HashMap;
use tracing;

/// A detected gap in a peer's attention chain.
#[derive(Debug, Clone)]
pub struct ChainGap {
    /// The author whose chain has a gap.
    pub author: MemberId,
    /// Our latest known sequence number for this author (0 if unknown).
    pub local_seq: u64,
    /// The remote tip's sequence number.
    pub remote_seq: u64,
    /// Number of events we're missing.
    pub missing_events: u64,
}

/// Compare local attention document state against tip advertisements to find gaps.
///
/// Returns a list of gaps where the tip document advertises a higher seq
/// than what we have locally in the attention document.
pub fn detect_gaps(
    attention_doc: &AttentionDocument,
    tip_doc: &AttentionTipDocument,
) -> Vec<ChainGap> {
    let mut gaps = Vec::new();

    for (author, tip) in tip_doc.all_tips() {
        let local_events = attention_doc.chain_events_for(author);
        let local_seq = local_events.last().map_or(0, |e| e.seq);

        if tip.seq > local_seq {
            gaps.push(ChainGap {
                author: *author,
                local_seq,
                remote_seq: tip.seq,
                missing_events: tip.seq - local_seq,
            });
        }
    }

    gaps
}

/// Validate all chains in an attention document after sync.
///
/// Returns a list of (author, error) for any chains that fail validation.
/// Chains are validated without signature checks (signatures are verified
/// on ingest, not on every read).
pub fn validate_synced_chains(
    attention_doc: &AttentionDocument,
) -> Vec<(MemberId, ValidationError)> {
    let mut errors = Vec::new();

    for (author, events) in attention_doc.all_chain_events() {
        if events.is_empty() {
            continue;
        }
        // Validate chain integrity (no signature check -- that happens on ingest)
        if let Err(e) = validate_chain(events, None) {
            tracing::warn!(
                author = hex::encode(author),
                error = %e,
                "chain validation failed after sync"
            );
            errors.push((*author, e));
        }
    }

    errors
}

/// Log detected gaps for observability.
pub fn log_gaps(gaps: &[ChainGap]) {
    for gap in gaps {
        tracing::info!(
            author = hex::encode(gap.author),
            local_seq = gap.local_seq,
            remote_seq = gap.remote_seq,
            missing = gap.missing_events,
            "attention chain gap detected -- CRDT sync will fill"
        );
    }
}

/// Reconstruct current attention state from chain events.
///
/// Validates each author's chain and extracts the final `AuthorState`,
/// which includes the current attention target. Returns a map from
/// author to their validated state. Authors whose chains fail validation
/// are excluded (and logged as warnings).
pub fn reconstruct_attention_state(
    attention_doc: &AttentionDocument,
) -> HashMap<MemberId, AuthorState> {
    let mut state = HashMap::new();

    for (author, events) in attention_doc.all_chain_events() {
        if events.is_empty() {
            continue;
        }
        match validate_chain(events, None) {
            Ok(author_state) => {
                state.insert(*author, author_state);
            }
            Err(e) => {
                tracing::warn!(
                    author = hex::encode(author),
                    error = %e,
                    "skipping author with invalid chain during state reconstruction"
                );
            }
        }
    }

    state
}

/// Extract a simple view of who is attending what from chain events.
///
/// Convenience wrapper around [`reconstruct_attention_state`] that returns
/// only the current attention targets, discarding seq/hash metadata.
pub fn current_attention_targets(
    attention_doc: &AttentionDocument,
) -> HashMap<MemberId, Option<ArtifactId>> {
    reconstruct_attention_state(attention_doc)
        .into_iter()
        .map(|(author, state)| (author, state.current_attention))
        .collect()
}

/// Full sync check: detect gaps, log them, and validate existing chains.
///
/// Returns the gaps found. Validation errors are logged as warnings.
/// In Phase 1, gap-filling happens automatically via `Document<T>` CRDT sync.
pub fn sync_attention_chains(
    attention_doc: &AttentionDocument,
    tip_doc: &AttentionTipDocument,
) -> Vec<ChainGap> {
    let gaps = detect_gaps(attention_doc, tip_doc);
    log_gaps(&gaps);

    // Validate chains we already have
    let errors = validate_synced_chains(attention_doc);
    if !errors.is_empty() {
        tracing::warn!(
            count = errors.len(),
            "chain validation errors found after sync"
        );
    }

    gaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attention_tip::AttentionTip;
    use indras_artifacts::attention::AttentionSwitchEvent as ChainedSwitchEvent;

    fn test_author(n: u8) -> MemberId {
        [n; 32]
    }

    #[test]
    fn test_detect_no_gaps_when_empty() {
        let doc = AttentionDocument::new();
        let tips = AttentionTipDocument::new();
        let gaps = detect_gaps(&doc, &tips);
        assert!(gaps.is_empty());
    }

    #[test]
    fn test_detect_gap_when_tip_ahead() {
        let doc = AttentionDocument::new();
        let mut tips = AttentionTipDocument::new();
        let author = test_author(1);

        tips.update_tip(AttentionTip {
            author,
            seq: 5,
            event_hash: [0u8; 32],
            wall_time_ms: 1000,
        });

        let gaps = detect_gaps(&doc, &tips);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].author, author);
        assert_eq!(gaps[0].local_seq, 0);
        assert_eq!(gaps[0].remote_seq, 5);
        assert_eq!(gaps[0].missing_events, 5);
    }

    #[test]
    fn test_no_gap_when_synced() {
        let mut doc = AttentionDocument::new();
        let mut tips = AttentionTipDocument::new();
        let author = test_author(1);

        // Add a chain event at seq 3
        let event = ChainedSwitchEvent::new(author, 3, 1000, None, None, [0u8; 32]);
        doc.store_chain_event(event.clone());

        // Tip also at seq 3
        tips.update_tip(AttentionTip {
            author,
            seq: 3,
            event_hash: event.event_hash(),
            wall_time_ms: 1000,
        });

        let gaps = detect_gaps(&doc, &tips);
        assert!(gaps.is_empty());
    }

    #[test]
    fn test_reconstruct_attention_state() {
        use indras_artifacts::artifact::ArtifactId;

        let mut doc = AttentionDocument::new();
        let author_a = test_author(1);
        let author_b = test_author(2);
        let artifact_x = ArtifactId::Blob([0xAA; 32]);
        let artifact_y = ArtifactId::Blob([0xBB; 32]);

        // Author A: genesis -> artifact_x -> artifact_y
        let a0 = ChainedSwitchEvent::new(author_a, 0, 1000, None, Some(artifact_x), [0u8; 32]);
        let h0 = a0.event_hash();
        let a1 = ChainedSwitchEvent::new(author_a, 1, 2000, Some(artifact_x), Some(artifact_y), h0);

        doc.store_chain_event(a0);
        doc.store_chain_event(a1);

        // Author B: genesis -> artifact_x (still there)
        let b0 = ChainedSwitchEvent::new(author_b, 0, 1500, None, Some(artifact_x), [0u8; 32]);
        doc.store_chain_event(b0);

        let state = reconstruct_attention_state(&doc);
        assert_eq!(state.len(), 2);
        assert_eq!(state[&author_a].current_attention, Some(artifact_y));
        assert_eq!(state[&author_a].latest_seq, 1);
        assert_eq!(state[&author_b].current_attention, Some(artifact_x));
        assert_eq!(state[&author_b].latest_seq, 0);
    }

    #[test]
    fn test_current_attention_targets() {
        use indras_artifacts::artifact::ArtifactId;

        let mut doc = AttentionDocument::new();
        let author = test_author(1);
        let artifact = ArtifactId::Blob([0xCC; 32]);

        let e0 = ChainedSwitchEvent::new(author, 0, 1000, None, Some(artifact), [0u8; 32]);
        doc.store_chain_event(e0);

        let targets = current_attention_targets(&doc);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[&author], Some(artifact));
    }

    #[test]
    fn test_reconstruct_skips_invalid_chain() {
        let mut doc = AttentionDocument::new();
        let author = test_author(1);

        // Store an event with seq=5 (not genesis) — invalid chain
        let bad = ChainedSwitchEvent::new(author, 5, 1000, None, None, [0u8; 32]);
        doc.store_chain_event(bad);

        let state = reconstruct_attention_state(&doc);
        assert!(state.is_empty(), "invalid chain should be excluded");
    }

    #[test]
    fn test_validate_valid_chain() {
        let mut doc = AttentionDocument::new();
        let author = test_author(1);

        // Build a valid 3-event chain
        let e0 = ChainedSwitchEvent::new(author, 0, 1000, None, None, [0u8; 32]);
        let h0 = e0.event_hash();
        let e1 = ChainedSwitchEvent::new(author, 1, 2000, None, None, h0);
        let h1 = e1.event_hash();
        let e2 = ChainedSwitchEvent::new(author, 2, 3000, None, None, h1);

        doc.store_chain_event(e0);
        doc.store_chain_event(e1);
        doc.store_chain_event(e2);

        let errors = validate_synced_chains(&doc);
        assert!(errors.is_empty());
    }
}
