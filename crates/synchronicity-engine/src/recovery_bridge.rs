//! Bridge for the Recovery Setup overlay.
//!
//! Re-derives the encryption subkey from the user's pass story and
//! produces a K-of-N steward recovery split via
//! [`indras_sync_engine::story_auth::StoryAuth::prepare_steward_recovery`].
//!
//! This is debug-grade Phase 1 plumbing — the user pastes each
//! steward's ML-KEM-768 encapsulation key as hex, and the encrypted
//! shares come back as hex strings to ship out-of-band. In-realm
//! distribution over iroh is a follow-on.

use std::sync::Arc;

use indras_crypto::pq_kem::PQEncapsulationKey;
use indras_crypto::story_template::PassStory;
use indras_network::IndrasNetwork;
use indras_sync_engine::peer_key_directory::PeerKeyDirectory;
use indras_sync_engine::story_auth::StoryAuth;
use indras_sync_engine::steward_recovery::StewardId;

/// Prepare a steward recovery split.
///
/// `story_slots` are the user's 23 pass-story slots (one entry per
/// slot, exactly 23). `stewards` is `(label, hex_ek)` pairs. On success
/// the manifest is persisted and one hex-encoded
/// `EncryptedStewardShare` is returned per steward in input order.
pub async fn setup_steward_recovery(
    story_slots: Vec<String>,
    stewards_input: Vec<(String, String)>,
    k: u8,
) -> Result<Vec<String>, String> {
    let data_dir = crate::state::default_data_dir();

    if story_slots.len() != 23 {
        return Err(format!(
            "Pass story must have exactly 23 slots (got {})",
            story_slots.len()
        ));
    }
    if let Some(empty_idx) = story_slots.iter().position(|s| s.trim().is_empty()) {
        return Err(format!("Pass story slot {} is empty", empty_idx + 1));
    }
    let slots: [String; 23] = story_slots
        .try_into()
        .map_err(|_: Vec<_>| "Failed to coerce 23 slots into array".to_string())?;
    let story = PassStory::from_normalized(slots).map_err(|e| format!("{}", e))?;

    let mut stewards: Vec<(StewardId, PQEncapsulationKey)> =
        Vec::with_capacity(stewards_input.len());
    for (label, hex_ek) in stewards_input {
        let label_trimmed = label.trim();
        if label_trimmed.is_empty() {
            return Err("Every steward needs a label".to_string());
        }
        let ek_bytes = hex::decode(hex_ek.trim())
            .map_err(|e| format!("Steward `{}`: invalid hex — {}", label_trimmed, e))?;
        let ek = PQEncapsulationKey::from_bytes(&ek_bytes)
            .map_err(|e| format!("Steward `{}`: invalid KEM key — {}", label_trimmed, e))?;
        stewards.push((StewardId::new(label_trimmed.as_bytes().to_vec()), ek));
    }

    let prepared = tokio::task::spawn_blocking(move || {
        StoryAuth::prepare_steward_recovery(&data_dir, &story, &stewards, k, 1)
    })
    .await
    .map_err(|e| format!("task join error: {}", e))?
    .map_err(|e| format!("{}", e))?;

    let mut shares_hex = Vec::with_capacity(prepared.encrypted_shares.len());
    for enc in &prepared.encrypted_shares {
        let bytes = enc
            .to_bytes()
            .map_err(|e| format!("serialize share: {}", e))?;
        shares_hex.push(hex::encode(bytes));
    }
    Ok(shares_hex)
}

/// Generate a fresh ML-KEM-768 keypair for testing the steward flow.
///
/// Returns `(decapsulation_key_hex, encapsulation_key_hex)` so the
/// caller can simulate a steward by pasting the encapsulation key into
/// the Recovery Setup overlay and keeping the decapsulation key for
/// later "release" of their share.
pub fn generate_test_steward_keypair() -> (String, String) {
    use indras_crypto::pq_kem::PQKemKeyPair;
    let kp = PQKemKeyPair::generate();
    let (dk, ek) = kp.to_keypair_bytes();
    (hex::encode(dk.as_slice()), hex::encode(ek))
}

/// A peer whose ML-KEM-768 key is available in the network, surfaced to
/// the Backup-plan UI as a one-click "add as friend" candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableSteward {
    /// Human-facing label — the peer's mirrored display name if we
    /// could resolve it from a DM realm, otherwise `Peer {uid_hex[..8]}`.
    pub label: String,
    /// Hex-encoded ML-KEM-768 encapsulation key.
    pub ek_hex: String,
}

/// Enumerate peers across every realm the user belongs to that have
/// published an ML-KEM-768 encapsulation key in the realm's peer-keys
/// directory. Deduped by `UserId`; the caller's own entry is skipped.
///
/// For DM realms, the single non-self peer's display name is looked up
/// via the profile mirror (`load_peer_profile_from_dm`) and used as the
/// label. In shared realms there is no 1:1 MemberId↔UserId mapping on
/// disk, so peers only visible through shared realms fall back to the
/// hex-prefix label until a proper directory lands.
pub async fn list_available_stewards(network: Arc<IndrasNetwork>) -> Vec<AvailableSteward> {
    use std::collections::BTreeMap;

    let my_uid = network.node().pq_identity().user_id();
    // uid -> (ek, optional resolved display name). First non-empty
    // name wins; ek from the first realm that publishes it wins.
    let mut found: BTreeMap<[u8; 32], (PQEncapsulationKey, Option<String>)> = BTreeMap::new();

    for realm_id in network.conversation_realms() {
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };
        let Ok(doc) = realm.document::<PeerKeyDirectory>("peer-keys").await else {
            continue;
        };
        let peers: Vec<([u8; 32], PQEncapsulationKey)> = {
            let data = doc.read().await;
            data.peers_with_kem()
                .into_iter()
                .filter(|(uid, _)| *uid != my_uid)
                .collect()
        };
        if peers.is_empty() {
            continue;
        }

        // DM realms have exactly one non-self member, so whatever peers
        // publish KEM keys in this realm's directory map to that one
        // MemberId. Shared realms return None here and fall through to
        // the hex fallback.
        let dm_name = match network.dm_peer_for_realm(&realm_id) {
            Some(mid) => {
                let realm_bytes = *realm_id.as_bytes();
                crate::profile_bridge::load_peer_profile_from_dm(&network, mid, realm_bytes)
                    .await
                    .and_then(|p| {
                        let t = p.display_name.trim();
                        if t.is_empty() { None } else { Some(t.to_string()) }
                    })
            }
            None => None,
        };

        for (uid, ek) in peers {
            found
                .entry(uid)
                .and_modify(|(_, n)| {
                    if n.is_none() {
                        *n = dm_name.clone();
                    }
                })
                .or_insert_with(|| (ek.clone(), dm_name.clone()));
        }
    }

    found
        .into_iter()
        .map(|(uid, (ek, name))| {
            let label = name.unwrap_or_else(|| {
                let uid_hex = hex::encode(uid);
                format!("Peer {}", &uid_hex[..8])
            });
            AvailableSteward {
                label,
                ek_hex: hex::encode(ek.to_bytes()),
            }
        })
        .collect()
}
