//! Witness selection for quorum certificates.
//!
//! Computes mutual peers between two players and selects eligible
//! witnesses with a Byzantine quorum threshold.
//!
//! # Algorithm
//!
//! Given players P and Q:
//! 1. Compute mutual peers M(P,Q) = N(P) intersection N(Q).
//! 2. If |M| >= m_min, the witness roster is M with threshold k = floor(|M|/2) + 1.

use crate::artifact::PlayerId;
use crate::peering::PeerRegistry;

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

/// Select eligible witnesses and compute the quorum threshold.
///
/// Given the mutual peer set and a minimum witness count `m_min`,
/// returns `Some((witnesses, k))` where `k = floor(|witnesses|/2) + 1`,
/// or `None` if the mutual set is too small.
pub fn select_witnesses(
    mutual: &[PlayerId],
    m_min: usize,
) -> Option<(Vec<PlayerId>, usize)> {
    if mutual.len() < m_min {
        return None;
    }
    let k = mutual.len() / 2 + 1;
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
    fn test_select_witnesses_sufficient() {
        let mutual: Vec<PlayerId> = (1..=5).map(test_player).collect();

        let result = select_witnesses(&mutual, 3);
        assert!(result.is_some());
        let (witnesses, k) = result.unwrap();
        assert_eq!(witnesses.len(), 5);
        assert_eq!(k, 3); // floor(5/2) + 1
    }

    #[test]
    fn test_select_witnesses_exact_minimum() {
        let mutual: Vec<PlayerId> = (1..=3).map(test_player).collect();

        let result = select_witnesses(&mutual, 3);
        assert!(result.is_some());
        let (witnesses, k) = result.unwrap();
        assert_eq!(witnesses.len(), 3);
        assert_eq!(k, 2); // floor(3/2) + 1
    }

    #[test]
    fn test_select_witnesses_insufficient() {
        let mutual: Vec<PlayerId> = (1..=2).map(test_player).collect();

        let result = select_witnesses(&mutual, 3);
        assert!(result.is_none());
    }

    #[test]
    fn test_select_witnesses_even_count() {
        let mutual: Vec<PlayerId> = (1..=4).map(test_player).collect();

        let result = select_witnesses(&mutual, 2);
        assert!(result.is_some());
        let (_, k) = result.unwrap();
        assert_eq!(k, 3); // floor(4/2) + 1
    }

    #[test]
    fn test_quorum_threshold_always_majority() {
        // Verify k > n/2 for various sizes
        for n in 1..=10 {
            let mutual: Vec<PlayerId> = (1..=n).map(|i| test_player(i as u8)).collect();
            if let Some((_, k)) = select_witnesses(&mutual, 1) {
                assert!(k * 2 > n, "k={k} should be strict majority of n={n}");
            }
        }
    }
}
