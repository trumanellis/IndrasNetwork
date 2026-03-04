//! Witness selection for quorum certificates.
//!
//! Computes mutual peers between two players and selects eligible
//! witnesses with a Byzantine fault-tolerant quorum threshold.
//!
//! # Algorithm
//!
//! Given players P and Q:
//! 1. Compute mutual peers M(P,Q) = N(P) intersection N(Q).
//! 2. If |M| >= m_min, the witness roster is M with BFT threshold:
//!    - `f = floor((n-1) / 3)` (max tolerable Byzantine faults)
//!    - `k = n - f` (quorum size guaranteeing overlap of f+1)
//!
//! This ensures two quorums overlap by at least `f+1` nodes, so even
//! with `f` Byzantine witnesses, at least one honest witness is in
//! both quorums. Requires `n >= 4` for any fault tolerance (`f >= 1`).

use crate::artifact::PlayerId;
use crate::peering::PeerRegistry;

/// Compute the BFT quorum threshold for a witness set of size `n`.
///
/// Returns `(f, k)` where:
/// - `f = floor((n-1) / 3)` — maximum tolerable Byzantine faults
/// - `k = n - f` — quorum size ensuring overlap of `f+1` nodes
///
/// Two quorums of size `k` from `n` nodes overlap by `2k - n = n - 2f`
/// nodes, which is at least `f + 1` (since `n >= 3f + 1` by construction).
/// With at most `f` Byzantine nodes, at least one overlap node is honest.
///
/// Notable thresholds:
/// - `n=1`: f=0, k=1 (trivial, no fault tolerance)
/// - `n=3`: f=0, k=3 (all must sign, no fault tolerance)
/// - `n=4`: f=1, k=3 (minimum for 1-fault tolerance)
/// - `n=7`: f=2, k=5 (minimum for 2-fault tolerance)
/// - `n=10`: f=3, k=7 (minimum for 3-fault tolerance)
pub fn bft_quorum_threshold(n: usize) -> (usize, usize) {
    if n == 0 {
        return (0, 0);
    }
    let f = (n - 1) / 3;
    let k = n - f;
    (f, k)
}

/// Compute mutual peers: the intersection of two peer registries.
///
/// Returns the set of players who appear in both `registry_p` and
/// `registry_q` (i.e., N(p) intersection N(q)).
pub fn mutual_peers(registry_p: &PeerRegistry, registry_q: &PeerRegistry) -> Vec<PlayerId> {
    registry_p
        .peers()
        .iter()
        .filter(|entry| registry_q.is_peer(&entry.peer_id))
        .map(|entry| entry.peer_id)
        .collect()
}

/// Select eligible witnesses and compute the BFT quorum threshold.
///
/// Given the mutual peer set and a minimum witness count `m_min`,
/// returns `Some((witnesses, k))` where `k = n - f` (BFT threshold),
/// or `None` if the mutual set is too small.
///
/// For Byzantine fault tolerance (f >= 1), require `m_min >= 4`.
pub fn select_witnesses(
    mutual: &[PlayerId],
    m_min: usize,
) -> Option<(Vec<PlayerId>, usize)> {
    if mutual.len() < m_min {
        return None;
    }
    let (_f, k) = bft_quorum_threshold(mutual.len());
    Some((mutual.to_vec(), k))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_player(n: u8) -> PlayerId {
        [n; 32]
    }

    fn registry_with_peers(owner: u8, peer_ids: &[u8]) -> PeerRegistry {
        let mut reg = PeerRegistry::new(test_player(owner));
        for &id in peer_ids {
            reg.add_peer(test_player(id), None, 1000).unwrap();
        }
        reg
    }

    #[test]
    fn test_mutual_peers_overlap() {
        let reg_p = registry_with_peers(1, &[2, 3, 4, 5]);
        let reg_q = registry_with_peers(6, &[3, 4, 5, 7]);

        let mutual = mutual_peers(&reg_p, &reg_q);
        assert_eq!(mutual.len(), 3);
        assert!(mutual.contains(&test_player(3)));
        assert!(mutual.contains(&test_player(4)));
        assert!(mutual.contains(&test_player(5)));
    }

    #[test]
    fn test_mutual_peers_no_overlap() {
        let reg_p = registry_with_peers(1, &[2, 3]);
        let reg_q = registry_with_peers(4, &[5, 6]);

        let mutual = mutual_peers(&reg_p, &reg_q);
        assert!(mutual.is_empty());
    }

    #[test]
    fn test_mutual_peers_empty_registries() {
        let reg_p = PeerRegistry::new(test_player(1));
        let reg_q = PeerRegistry::new(test_player(2));

        let mutual = mutual_peers(&reg_p, &reg_q);
        assert!(mutual.is_empty());
    }

    #[test]
    fn test_bft_quorum_threshold_known_values() {
        assert_eq!(bft_quorum_threshold(0), (0, 0));
        assert_eq!(bft_quorum_threshold(1), (0, 1));
        assert_eq!(bft_quorum_threshold(3), (0, 3));
        assert_eq!(bft_quorum_threshold(4), (1, 3)); // min for f=1
        assert_eq!(bft_quorum_threshold(5), (1, 4));
        assert_eq!(bft_quorum_threshold(7), (2, 5)); // min for f=2
        assert_eq!(bft_quorum_threshold(10), (3, 7)); // min for f=3
    }

    #[test]
    fn test_bft_quorum_overlap_guarantee() {
        // For all n, verify overlap = 2k - n >= f + 1
        for n in 1..=30 {
            let (f, k) = bft_quorum_threshold(n);
            let overlap = 2 * k - n;
            assert!(
                overlap >= f + 1,
                "n={n}, f={f}, k={k}, overlap={overlap} < f+1={}",
                f + 1,
            );
        }
    }

    #[test]
    fn test_select_witnesses_sufficient() {
        let mutual: Vec<PlayerId> = (1..=5).map(test_player).collect();

        let result = select_witnesses(&mutual, 3);
        assert!(result.is_some());
        let (witnesses, k) = result.unwrap();
        assert_eq!(witnesses.len(), 5);
        assert_eq!(k, 4); // n=5, f=1, k=n-f=4
    }

    #[test]
    fn test_select_witnesses_exact_minimum() {
        let mutual: Vec<PlayerId> = (1..=3).map(test_player).collect();

        let result = select_witnesses(&mutual, 3);
        assert!(result.is_some());
        let (witnesses, k) = result.unwrap();
        assert_eq!(witnesses.len(), 3);
        assert_eq!(k, 3); // n=3, f=0, k=n-f=3 (all must sign)
    }

    #[test]
    fn test_select_witnesses_insufficient() {
        let mutual: Vec<PlayerId> = (1..=2).map(test_player).collect();

        let result = select_witnesses(&mutual, 3);
        assert!(result.is_none());
    }

    #[test]
    fn test_select_witnesses_four_gives_fault_tolerance() {
        let mutual: Vec<PlayerId> = (1..=4).map(test_player).collect();

        let result = select_witnesses(&mutual, 4);
        assert!(result.is_some());
        let (_, k) = result.unwrap();
        assert_eq!(k, 3); // n=4, f=1, k=3 — minimum for 1-fault tolerance
    }

    #[test]
    fn test_quorum_threshold_always_majority() {
        // BFT threshold is always at least strict majority
        for n in 1..=20 {
            let (_, k) = bft_quorum_threshold(n);
            assert!(k * 2 > n, "k={k} should be strict majority of n={n}");
        }
    }
}
