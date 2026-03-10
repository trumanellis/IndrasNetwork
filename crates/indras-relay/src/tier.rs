//! Tier determination and tier-aware quota helpers
//!
//! Maps authenticated peers to storage tiers based on their relationship
//! to the relay owner. The three tiers are:
//! - **Self**: Owner's own data (backup, pinning, cross-device sync)
//! - **Connections**: Mutual peers (realm sync, encrypted S&F, custody)
//! - **Public**: Network broadcast (announcements, discovery)

use indras_transport::protocol::StorageTier;

use crate::config::TierConfig;

/// Determine the highest tier a peer should receive.
///
/// - If `player_id == owner_id`, returns `Self_`.
/// - If `player_id` is in the owner's contacts list, returns `Connections`.
/// - Otherwise returns `Public`.
///
/// When `owner_id` is `None` (community mode), all authenticated peers
/// get `Public` tier unless explicitly granted via contacts.
pub fn determine_tier(
    player_id: &[u8; 32],
    owner_id: Option<&[u8; 32]>,
    contacts: &[[u8; 32]],
) -> StorageTier {
    if let Some(owner) = owner_id {
        if player_id == owner {
            return StorageTier::Self_;
        }
    }

    if contacts.contains(player_id) {
        return StorageTier::Connections;
    }

    StorageTier::Public
}

/// Return all tiers a peer has access to (inclusive of lower tiers).
///
/// Self_ grants access to all three tiers.
/// Connections grants Connections + Public.
/// Public grants only Public.
pub fn granted_tiers(highest: StorageTier) -> Vec<StorageTier> {
    match highest {
        StorageTier::Self_ => vec![
            StorageTier::Self_,
            StorageTier::Connections,
            StorageTier::Public,
        ],
        StorageTier::Connections => vec![StorageTier::Connections, StorageTier::Public],
        StorageTier::Public => vec![StorageTier::Public],
    }
}

/// Get the max bytes limit for a specific tier from config.
pub fn tier_max_bytes(tier: StorageTier, config: &TierConfig) -> u64 {
    match tier {
        StorageTier::Self_ => config.self_max_bytes,
        StorageTier::Connections => config.connections_max_bytes,
        StorageTier::Public => config.public_max_bytes,
    }
}

/// Get the TTL in days for a specific tier from config.
pub fn tier_ttl_days(tier: StorageTier, config: &TierConfig) -> u64 {
    match tier {
        StorageTier::Self_ => config.self_ttl_days,
        StorageTier::Connections => config.connections_ttl_days,
        StorageTier::Public => config.public_ttl_days,
    }
}

/// Get the max interfaces for a specific tier from config.
pub fn tier_max_interfaces(tier: StorageTier, config: &TierConfig) -> usize {
    match tier {
        StorageTier::Self_ => config.self_max_interfaces,
        StorageTier::Connections => config.connections_max_interfaces,
        StorageTier::Public => config.public_max_interfaces,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_owner_gets_self_tier() {
        let owner = [1u8; 32];
        let tier = determine_tier(&owner, Some(&owner), &[]);
        assert_eq!(tier, StorageTier::Self_);
    }

    #[test]
    fn test_contact_gets_connections_tier() {
        let owner = [1u8; 32];
        let contact = [2u8; 32];
        let tier = determine_tier(&contact, Some(&owner), &[contact]);
        assert_eq!(tier, StorageTier::Connections);
    }

    #[test]
    fn test_stranger_gets_public_tier() {
        let owner = [1u8; 32];
        let stranger = [3u8; 32];
        let tier = determine_tier(&stranger, Some(&owner), &[]);
        assert_eq!(tier, StorageTier::Public);
    }

    #[test]
    fn test_community_mode_no_owner() {
        let peer = [2u8; 32];
        // No owner — community mode
        let tier = determine_tier(&peer, None, &[]);
        assert_eq!(tier, StorageTier::Public);

        // But contacts still get Connections
        let tier = determine_tier(&peer, None, &[peer]);
        assert_eq!(tier, StorageTier::Connections);
    }

    #[test]
    fn test_granted_tiers_self() {
        let tiers = granted_tiers(StorageTier::Self_);
        assert_eq!(tiers.len(), 3);
        assert!(tiers.contains(&StorageTier::Self_));
        assert!(tiers.contains(&StorageTier::Connections));
        assert!(tiers.contains(&StorageTier::Public));
    }

    #[test]
    fn test_granted_tiers_connections() {
        let tiers = granted_tiers(StorageTier::Connections);
        assert_eq!(tiers.len(), 2);
        assert!(!tiers.contains(&StorageTier::Self_));
    }

    #[test]
    fn test_granted_tiers_public() {
        let tiers = granted_tiers(StorageTier::Public);
        assert_eq!(tiers.len(), 1);
    }

    #[test]
    fn test_tier_config_lookups() {
        let config = TierConfig::default();
        assert_eq!(tier_max_bytes(StorageTier::Self_, &config), 1024 * 1024 * 1024);
        assert_eq!(tier_max_bytes(StorageTier::Connections, &config), 500 * 1024 * 1024);
        assert_eq!(tier_max_bytes(StorageTier::Public, &config), 50 * 1024 * 1024);
        assert_eq!(tier_ttl_days(StorageTier::Self_, &config), 365);
        assert_eq!(tier_ttl_days(StorageTier::Connections, &config), 90);
        assert_eq!(tier_ttl_days(StorageTier::Public, &config), 7);
    }
}
