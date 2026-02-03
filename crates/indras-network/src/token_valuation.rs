//! Subjective token valuation — value depends on who's looking.
//!
//! The same Token of Gratitude can be worth different amounts to different
//! observers. Value is computed as:
//!
//!   `raw_attention_millis × trust_chain_weight × humanness_freshness`
//!
//! Where:
//! - `raw_attention_millis` comes from the token's backing attention events
//! - `trust_chain_weight` is derived from the observer's sentiment toward
//!   members in the token's steward chain, decayed by distance
//! - `humanness_freshness` measures how recently the blesser was attested
//!   as human (see `humanness` module)
//!
//! Only positive sentiment counts — negative and unknown sentiment produce
//! a weight of zero. This means sybil-minted tokens are invisible to
//! observers who don't trust the sybil accounts.

use crate::member::MemberId;
use crate::token_of_gratitude::TokenOfGratitude;

/// Trust decay factor per hop in the steward chain.
///
/// Each additional hop from a trusted steward reduces the trust weight
/// by this factor. At 0.7: 1 hop = 0.7, 2 hops = 0.49, 3 hops = 0.343.
pub const STEWARD_CHAIN_DECAY: f64 = 0.7;

/// Result of subjective token valuation.
#[derive(Debug, Clone, PartialEq)]
pub struct SubjectiveTokenValue {
    /// Raw attention duration in milliseconds (objective, same for all observers).
    pub raw_attention_millis: u64,
    /// Trust weight from steward chain analysis (0.0 to 1.0).
    pub trust_chain_weight: f64,
    /// Humanness freshness of the blesser (0.0 to 1.0).
    pub humanness_freshness: f64,
    /// Final subjective value: raw × trust × freshness.
    pub subjective_millis: f64,
}

/// Compute the subjective value of a token from an observer's perspective.
///
/// # Algorithm
///
/// 1. Scan the blesser and every member in the steward chain
/// 2. For each, look up the observer's sentiment (via `sentiment_fn`)
/// 3. Only positive sentiment counts (negative/unknown → 0.0)
/// 4. Find the strongest positive sentiment, apply chain decay based on
///    distance from that trusted member to the current steward
/// 5. Multiply by humanness freshness of the blesser
///
/// # Arguments
///
/// * `token` - The token to evaluate
/// * `raw_attention_millis` - Pre-computed attention duration backing the token
/// * `sentiment_fn` - Returns the observer's sentiment toward a member (None = unknown, Some(score) where score is -1.0 to 1.0)
/// * `humanness_fn` - Returns humanness freshness for a member (0.0 to 1.0)
pub fn subjective_value(
    token: &TokenOfGratitude,
    raw_attention_millis: u64,
    sentiment_fn: impl Fn(&MemberId) -> Option<f64>,
    humanness_fn: impl Fn(&MemberId) -> f64,
) -> SubjectiveTokenValue {
    let chain = &token.steward_chain;
    let chain_len = chain.len();

    // Find the best trust weight by scanning blesser + steward chain.
    // For each member we know positively, compute decayed trust based on
    // their distance from the end of the chain (current steward).
    let mut best_weight: f64 = 0.0;

    // Check the blesser first (not necessarily in the chain)
    if let Some(score) = sentiment_fn(&token.blesser) {
        let positive = score.max(0.0);
        if positive > 0.0 {
            // Blesser is at the origin — decay from position 0 to end
            let hops = chain_len.saturating_sub(1) as f64;
            let weight = positive * STEWARD_CHAIN_DECAY.powf(hops);
            best_weight = best_weight.max(weight);
        }
    }

    // Check each steward in the chain
    for (i, member) in chain.iter().enumerate() {
        if let Some(score) = sentiment_fn(member) {
            let positive = score.max(0.0);
            if positive > 0.0 {
                // Hops from this position to the current steward (end of chain)
                let hops = (chain_len - 1 - i) as f64;
                let weight = positive * STEWARD_CHAIN_DECAY.powf(hops);
                best_weight = best_weight.max(weight);
            }
        }
    }

    let freshness = humanness_fn(&token.blesser);
    let subjective = raw_attention_millis as f64 * best_weight * freshness;

    SubjectiveTokenValue {
        raw_attention_millis,
        trust_chain_weight: best_weight,
        humanness_freshness: freshness,
        subjective_millis: subjective,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blessing::BlessingId;
    use crate::quest::QuestId;
    use crate::token_of_gratitude::TokenOfGratitudeDocument;

    fn member(n: u8) -> MemberId {
        [n; 32]
    }
    fn quest(n: u8) -> QuestId {
        [n; 16]
    }
    fn blessing(n: u8) -> BlessingId {
        [n; 16]
    }

    /// Helper: mint a token and optionally release it through a chain of stewards.
    fn make_token_with_chain(stewards: &[MemberId], blesser: MemberId) -> TokenOfGratitude {
        let mut doc = TokenOfGratitudeDocument::new();
        let first = stewards[0];
        let token_id = doc
            .mint(first, blessing(1), blesser, quest(1), vec![0, 1, 2])
            .unwrap();

        for i in 1..stewards.len() {
            doc.pledge(token_id, quest(i as u8 + 1)).unwrap();
            doc.release(token_id, stewards[i]).unwrap();
        }

        doc.find(&token_id).unwrap().clone()
    }

    #[test]
    fn test_sybil_tokens_worth_zero() {
        // Sybil blesser and steward — observer has no sentiment toward them
        let token = make_token_with_chain(&[member(10)], member(11));

        let val = subjective_value(
            &token,
            60_000,
            |_| None, // no sentiment toward anyone
            |_| 1.0,  // assume fresh
        );

        assert_eq!(val.trust_chain_weight, 0.0);
        assert_eq!(val.subjective_millis, 0.0);
    }

    #[test]
    fn test_trusted_blesser_full_value() {
        // Observer directly trusts the blesser with score 1.0
        // Single steward (no hops from blesser perspective, but chain has 1 entry)
        let blesser = member(2);
        let token = make_token_with_chain(&[member(1)], blesser);

        let val = subjective_value(
            &token,
            60_000,
            |m| if *m == blesser { Some(1.0) } else { None },
            |_| 1.0,
        );

        // chain_len=1, hops from blesser = chain_len-1 = 0
        // weight = 1.0 * 0.7^0 = 1.0
        assert!((val.trust_chain_weight - 1.0).abs() < f64::EPSILON);
        assert!((val.subjective_millis - 60_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_friend_of_friend_decay() {
        // Token minted to A, released to B. Observer trusts A (score 1.0).
        // A is at chain[0], B is at chain[1]. Hops from A to current = 1.
        let a = member(1);
        let b = member(2);
        let blesser = member(3);
        let token = make_token_with_chain(&[a, b], blesser);

        let val = subjective_value(
            &token,
            60_000,
            |m| if *m == a { Some(1.0) } else { None },
            |_| 1.0,
        );

        // weight = 1.0 * 0.7^1 = 0.7
        assert!((val.trust_chain_weight - 0.7).abs() < 0.001);
        assert!((val.subjective_millis - 42_000.0).abs() < 1.0);
    }

    #[test]
    fn test_chain_decay_across_multiple_hops() {
        // A -> B -> C -> D. Observer trusts A.
        let a = member(1);
        let b = member(2);
        let c = member(3);
        let d = member(4);
        let blesser = member(5);
        let token = make_token_with_chain(&[a, b, c, d], blesser);

        let val = subjective_value(
            &token,
            100_000,
            |m| if *m == a { Some(1.0) } else { None },
            |_| 1.0,
        );

        // A at index 0, chain_len=4, hops = 4-1-0 = 3
        // weight = 1.0 * 0.7^3 = 0.343
        assert!((val.trust_chain_weight - 0.343).abs() < 0.001);
    }

    #[test]
    fn test_negative_sentiment_ignored() {
        // Observer has negative sentiment toward blesser
        let blesser = member(2);
        let token = make_token_with_chain(&[member(1)], blesser);

        let val = subjective_value(
            &token,
            60_000,
            |m| if *m == blesser { Some(-0.5) } else { None },
            |_| 1.0,
        );

        // Negative clamped to 0
        assert_eq!(val.trust_chain_weight, 0.0);
        assert_eq!(val.subjective_millis, 0.0);
    }

    #[test]
    fn test_stale_humanness_reduces_value() {
        let blesser = member(2);
        let token = make_token_with_chain(&[member(1)], blesser);

        let val = subjective_value(
            &token,
            60_000,
            |m| if *m == blesser { Some(1.0) } else { None },
            |_| 0.5, // half-fresh
        );

        assert!((val.trust_chain_weight - 1.0).abs() < f64::EPSILON);
        assert!((val.humanness_freshness - 0.5).abs() < f64::EPSILON);
        assert!((val.subjective_millis - 30_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_best_trust_in_chain_wins() {
        // A -> B -> C. Observer trusts B with 0.9, trusts A with 0.5.
        // B is closer (1 hop), so B should win: 0.9 * 0.7^1 = 0.63
        // vs A: 0.5 * 0.7^2 = 0.245
        let a = member(1);
        let b = member(2);
        let c = member(3);
        let blesser = member(4);
        let token = make_token_with_chain(&[a, b, c], blesser);

        let val = subjective_value(
            &token,
            100_000,
            |m| {
                if *m == a {
                    Some(0.5)
                } else if *m == b {
                    Some(0.9)
                } else {
                    None
                }
            },
            |_| 1.0,
        );

        // B at index 1, chain_len=3, hops = 3-1-1 = 1
        // weight = 0.9 * 0.7^1 = 0.63
        assert!((val.trust_chain_weight - 0.63).abs() < 0.001);
    }

    #[test]
    fn test_direct_steward_full_trust() {
        // Observer trusts the current steward directly (last in chain)
        let a = member(1);
        let b = member(2);
        let blesser = member(3);
        let token = make_token_with_chain(&[a, b], blesser);

        let val = subjective_value(
            &token,
            60_000,
            |m| if *m == b { Some(0.8) } else { None },
            |_| 1.0,
        );

        // B at index 1 (last), hops = 2-1-1 = 0
        // weight = 0.8 * 0.7^0 = 0.8
        assert!((val.trust_chain_weight - 0.8).abs() < 0.001);
    }
}
