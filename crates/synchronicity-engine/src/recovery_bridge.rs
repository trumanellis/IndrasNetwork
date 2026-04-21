//! Bridge for the Recovery Setup and Recovery Use overlays.
//!
//! **Backup side** (`setup_steward_recovery`): re-derives the
//! encryption subkey from the user's pass story and produces a K-of-N
//! steward recovery split via
//! [`indras_sync_engine::story_auth::StoryAuth::prepare_steward_recovery`].
//!
//! **Recovery side** (`use_steward_recovery`): collects K encrypted
//! shares plus their matching steward keypairs, decrypts each,
//! reassembles the subkey, and re-authenticates the local
//! `StoryKeystore` to prove the recovered identity unlocks the at-rest
//! PQ keys.
//!
//! This is debug-grade Phase 1 plumbing — the user pastes hex in and
//! hex out. In-realm share distribution over iroh is a follow-on.

use std::sync::Arc;

use indras_crypto::pq_kem::{PQEncapsulationKey, PQKemKeyPair};
use indras_crypto::steward_share::EncryptedStewardShare;
use indras_crypto::story_template::PassStory;
use indras_network::IndrasNetwork;
use indras_node::StoryKeystore;
use indras_sync_engine::peer_key_directory::PeerKeyDirectory;
use indras_sync_engine::story_auth::StoryAuth;
use indras_sync_engine::steward_recovery::{self, StewardId};

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

/// One steward's contribution to recovery, in the raw hex form the
/// debug UI collects. For Phase 1, the overlay decrypts on the user's
/// behalf using supplied keypairs; in-band delivery of pre-decrypted
/// shares lands with Slice 4.
#[derive(Clone, Debug, Default)]
pub struct RecoveryContribution {
    /// Hex-encoded `EncryptedStewardShare`.
    pub share_hex: String,
    /// Hex-encoded steward decapsulation key.
    pub decap_key_hex: String,
    /// Hex-encoded steward encapsulation key. Needed alongside
    /// `decap_key_hex` to reconstruct the `PQKemKeyPair` for decryption.
    pub encap_key_hex: String,
}

/// Rebuild the keystore encryption subkey from K contributions and
/// re-authenticate the local `StoryKeystore`, proving the recovered
/// identity can unlock the at-rest PQ keys. Returns the K used on
/// success for the UI's confirmation string.
///
/// Errors surface as human-readable strings — the overlay renders them
/// directly. Debug-grade: the caller does not need to paste the
/// verification token because it already lives on disk at
/// `<data_dir>/story.token`, left behind from account creation.
pub async fn use_steward_recovery(
    contributions: Vec<RecoveryContribution>,
    threshold: u8,
) -> Result<u8, String> {
    if threshold < 2 {
        return Err(format!("Threshold must be at least 2 (got {})", threshold));
    }
    if contributions.len() < threshold as usize {
        return Err(format!(
            "Need at least {} pieces; got {}",
            threshold,
            contributions.len()
        ));
    }

    let data_dir = crate::state::default_data_dir();

    tokio::task::spawn_blocking(move || -> Result<u8, String> {
        let mut shares = Vec::with_capacity(contributions.len());
        for (idx, c) in contributions.iter().enumerate() {
            let slot = idx + 1;
            let share_bytes = hex::decode(c.share_hex.trim())
                .map_err(|e| format!("Piece {}: hex decode failed — {}", slot, e))?;
            let encrypted = EncryptedStewardShare::from_bytes(&share_bytes)
                .map_err(|e| format!("Piece {}: malformed share — {}", slot, e))?;
            let dk_bytes = hex::decode(c.decap_key_hex.trim())
                .map_err(|e| format!("Piece {}: friend-secret hex decode failed — {}", slot, e))?;
            let ek_bytes = hex::decode(c.encap_key_hex.trim())
                .map_err(|e| format!("Piece {}: friend-code hex decode failed — {}", slot, e))?;
            let kp = PQKemKeyPair::from_keypair_bytes(&dk_bytes, &ek_bytes)
                .map_err(|e| format!("Piece {}: key pair rebuild failed — {}", slot, e))?;
            let share = encrypted
                .decrypt(&kp)
                .map_err(|e| format!("Piece {}: decrypt failed — {}", slot, e))?;
            shares.push(share);
        }

        let subkey = steward_recovery::recover_encryption_subkey(&shares, threshold)
            .map_err(|e| format!("Couldn't reassemble the backup: {}", e))?;

        let token_path = data_dir.join("story.token");
        let token_bytes = std::fs::read(&token_path)
            .map_err(|e| format!("No keystore on this device ({}): {}", token_path.display(), e))?;
        if token_bytes.len() != 32 {
            return Err(format!(
                "Keystore token is the wrong size (got {}, want 32)",
                token_bytes.len()
            ));
        }
        let mut token = [0u8; 32];
        token.copy_from_slice(&token_bytes);

        let mut keystore = StoryKeystore::new(&data_dir);
        if !keystore.is_initialized() {
            return Err("No keystore to recover on this device".to_string());
        }
        keystore
            .authenticate(&subkey, token)
            .map_err(|e| format!("Recovered key did not unlock the keystore: {}", e))?;
        keystore
            .load_or_generate_pq_identity()
            .map_err(|e| format!("Couldn't load the recovered identity: {}", e))?;

        Ok(threshold)
    })
    .await
    .map_err(|e| format!("task join error: {}", e))?
}
