//! Bridge for the Recovery Setup and Recovery Use overlays.
//!
//! **Backup side** (`setup_steward_recovery`): re-derives the
//! encryption subkey from the user's pass story (or loads it from the
//! on-disk cache populated at sign-in) and produces a K-of-N steward
//! recovery split via [`indras_sync_engine::steward_recovery`].
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
use indras_sync_engine::share_delivery::{
    share_delivery_doc_key, HeldBackup, ShareDelivery, StewardHoldings,
};
use indras_sync_engine::steward_enrollment::{
    invite_doc_key, response_doc_key, EnrollmentStatus, StewardInvitation, StewardResponse,
    DEFAULT_RESPONSIBILITY,
};
use indras_sync_engine::steward_recovery::{self, StewardId};

/// Filename of the encryption-subkey cache. Kept next to the PQ
/// signing key (which is already plaintext at rest in this dev tier),
/// so the on-disk exposure is comparable.
const SUBKEY_CACHE_FILENAME: &str = "story.subkey";

/// Persist the 32-byte story-derived encryption subkey so returning
/// users can set up a backup without re-entering their pass story.
pub fn save_subkey_cache(data_dir: &std::path::Path, subkey: &[u8; 32]) -> std::io::Result<()> {
    std::fs::write(data_dir.join(SUBKEY_CACHE_FILENAME), subkey)
}

/// Load the cached subkey, or `None` if it has not been written yet.
pub fn load_subkey_cache(data_dir: &std::path::Path) -> Option<[u8; 32]> {
    let bytes = std::fs::read(data_dir.join(SUBKEY_CACHE_FILENAME)).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

/// Delete the cached subkey. Called when the user signs out.
pub fn clear_subkey_cache(data_dir: &std::path::Path) {
    let _ = std::fs::remove_file(data_dir.join(SUBKEY_CACHE_FILENAME));
}

/// Convenience: is a subkey cached for the default data dir?
pub fn has_cached_subkey() -> bool {
    load_subkey_cache(&crate::state::default_data_dir()).is_some()
}

/// Re-derive the encryption subkey from a pass story, verifying the
/// result against the keystore's on-disk token so a mistyped story
/// can't produce garbage shares.
///
/// For initialized keystores (the normal case) we check the derived
/// token matches the stored one and bail if it doesn't.
///
/// For uninitialized keystores — Phase-1 accounts that were created
/// before the story-binding fix landed — we bootstrap: generate a
/// fresh salt, derive, and persist salt+token so future backup
/// setups can verify. Existing plaintext PQ keys are left untouched;
/// we only write the story token/salt.
fn derive_subkey_from_story(
    data_dir: &std::path::Path,
    story: &PassStory,
) -> Option<[u8; 32]> {
    use indras_crypto::pass_story::{derive_master_key, expand_subkeys, story_verification_token};
    use indras_node::StoryKeystore;

    let keystore = StoryKeystore::new(data_dir);
    let canonical = story.canonical().ok()?;

    if keystore.is_initialized() {
        let salt = keystore.load_story_salt().ok()?;
        let master = derive_master_key(&canonical, &salt).ok()?;
        let token = story_verification_token(&master);
        if !keystore.verify_token(&token).ok()? {
            return None;
        }
        let subkeys = expand_subkeys(&master).ok()?;
        return subkeys.encryption.as_slice().try_into().ok();
    }

    // Bootstrap path — build a salt from a per-device marker + the
    // current timestamp. The salt file is what makes the derivation
    // reproducible, so stability comes from writing it once here.
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let mut salt = Vec::with_capacity(16);
    salt.extend_from_slice(b"indras-p1");
    salt.extend_from_slice(&timestamp.to_le_bytes());

    let master = derive_master_key(&canonical, &salt).ok()?;
    let token = story_verification_token(&master);
    let subkeys = expand_subkeys(&master).ok()?;
    let arr: [u8; 32] = subkeys.encryption.as_slice().try_into().ok()?;

    // Persist the salt + token only (leave existing plaintext PQ
    // keys alone). Uses low-level file writes so we don't kick off
    // re-encryption of keys that were generated without the subkey.
    let _ = std::fs::write(data_dir.join("story.salt"), &salt);
    let _ = std::fs::write(data_dir.join("story.token"), token);
    drop(keystore);

    Some(arr)
}

/// One backup-plan row as handed down from the Backup-plan overlay.
///
/// `user_id_hex` is `Some` only for rows created via the peer picker —
/// those are candidates for in-band delivery because we already know
/// which DM realm reaches that peer. Manual hex-paste rows have `None`
/// and always fall through to the hex copy-paste path.
#[derive(Clone, Debug, Default)]
pub struct StewardInput {
    pub label: String,
    pub ek_hex: String,
    pub user_id_hex: Option<String>,
}

/// Summary of `setup_steward_recovery` for the UI.
///
/// `shares_hex` is the hex fallback every steward sees today. `delivered_to`
/// names the stewards whose shares were also published into a DM realm
/// over iroh (no hex copy-paste needed for those).
#[derive(Clone, Debug, Default)]
pub struct SetupOutcome {
    pub shares_hex: Vec<String>,
    pub delivered_to: Vec<String>,
}

/// Prepare a steward recovery split.
///
/// `story_slots` may be empty — when it is, the bridge falls back to
/// the cached encryption subkey on disk (populated at sign-in). This
/// lets returning users set up a backup without re-entering the
/// 23-word story. Passing a full 23-slot story re-derives fresh and
/// also refreshes the cache. For each input steward we encrypt a
/// share; when `network` is `Some` and the row carries a
/// `user_id_hex` for a peer we share a DM realm with, we also
/// publish the share as a CRDT doc inside that realm. Manifest is
/// persisted either way.
pub async fn setup_steward_recovery(
    story_slots: Vec<String>,
    stewards_input: Vec<StewardInput>,
    k: u8,
    network: Option<Arc<IndrasNetwork>>,
) -> Result<SetupOutcome, String> {
    let data_dir = crate::state::default_data_dir();

    // Normalize the story input: either fully populated (23 non-empty
    // slots) or fully empty (fall back to cached subkey). Anything in
    // between is a user error.
    let any_filled = story_slots.iter().any(|s| !s.trim().is_empty());
    let all_filled = story_slots.len() == 23
        && story_slots.iter().all(|s| !s.trim().is_empty());
    let story: Option<PassStory> = if all_filled {
        let slots: [String; 23] = story_slots
            .try_into()
            .map_err(|_: Vec<_>| "Failed to coerce 23 slots into array".to_string())?;
        Some(PassStory::from_normalized(slots).map_err(|e| format!("{}", e))?)
    } else if !any_filled {
        None
    } else {
        return Err("Pass story must be either fully filled or left empty.".to_string());
    };

    let mut stewards: Vec<(StewardId, PQEncapsulationKey)> =
        Vec::with_capacity(stewards_input.len());
    let mut routing: Vec<Option<[u8; 32]>> = Vec::with_capacity(stewards_input.len());
    let mut labels: Vec<String> = Vec::with_capacity(stewards_input.len());
    for input in &stewards_input {
        let label_trimmed = input.label.trim();
        if label_trimmed.is_empty() {
            return Err("Every steward needs a label".to_string());
        }
        let ek_bytes = hex::decode(input.ek_hex.trim())
            .map_err(|e| format!("Steward `{}`: invalid hex — {}", label_trimmed, e))?;
        let ek = PQEncapsulationKey::from_bytes(&ek_bytes)
            .map_err(|e| format!("Steward `{}`: invalid KEM key — {}", label_trimmed, e))?;
        stewards.push((StewardId::new(label_trimmed.as_bytes().to_vec()), ek));
        labels.push(label_trimmed.to_string());

        let uid = input
            .user_id_hex
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|hex_uid| {
                let bytes = hex::decode(hex_uid).map_err(|e| {
                    format!("Steward `{}`: invalid user_id hex — {}", label_trimmed, e)
                })?;
                if bytes.len() != 32 {
                    return Err(format!(
                        "Steward `{}`: user_id must be 32 bytes, got {}",
                        label_trimmed,
                        bytes.len()
                    ));
                }
                let mut out = [0u8; 32];
                out.copy_from_slice(&bytes);
                Ok::<_, String>(out)
            })
            .transpose()?;
        routing.push(uid);
    }

    let subkey = if let Some(story) = story {
        let data_dir_owned = data_dir.clone();
        tokio::task::spawn_blocking(move || derive_subkey_from_story(&data_dir_owned, &story))
            .await
            .map_err(|e| format!("task join error: {}", e))?
            .ok_or_else(|| {
                "Couldn't derive the backup key from that story — it may not match the keystore"
                    .to_string()
            })?
    } else {
        load_subkey_cache(&data_dir).ok_or_else(|| {
            "We don't have your secret story cached. Open section 01 and paste the words one time."
                .to_string()
        })?
    };

    // Always refresh the cache so subsequent setups can skip the story.
    let _ = save_subkey_cache(&data_dir, &subkey);

    let prepared = {
        let stewards = stewards.clone();
        let data_dir = data_dir.clone();
        tokio::task::spawn_blocking(move || -> Result<_, String> {
            let prepared = steward_recovery::prepare_recovery(&subkey, &stewards, k, 1)
                .map_err(|e| format!("{}", e))?;
            steward_recovery::save_manifest(&data_dir, &prepared.manifest)
                .map_err(|e| format!("{}", e))?;
            Ok(prepared)
        })
        .await
        .map_err(|e| format!("task join error: {}", e))??
    };

    let mut shares_hex = Vec::with_capacity(prepared.encrypted_shares.len());
    let mut share_bytes: Vec<Vec<u8>> = Vec::with_capacity(prepared.encrypted_shares.len());
    for enc in &prepared.encrypted_shares {
        let bytes = enc
            .to_bytes()
            .map_err(|e| format!("serialize share: {}", e))?;
        shares_hex.push(hex::encode(&bytes));
        share_bytes.push(bytes);
    }

    // In-band delivery: for each row whose caller provided a UserId,
    // publish the encrypted share into the sender↔steward DM realm.
    // Failures here are recorded in `delivered_to` absence, not as
    // hard errors — the hex fallback still lets the user ship out of
    // band.
    let mut delivered_to: Vec<String> = Vec::new();
    if let Some(net) = network {
        let my_uid = net.node().pq_identity().user_id();
        let now = chrono::Utc::now().timestamp_millis();
        let uid_to_realm = dm_realm_map(&net).await;
        for (idx, maybe_uid) in routing.iter().enumerate() {
            let Some(target_uid) = maybe_uid else { continue };
            let Some(realm_id) = uid_to_realm.get(target_uid) else { continue };
            let Some(realm) = net.get_realm_by_id(realm_id) else { continue };
            let key = share_delivery_doc_key(&my_uid);
            let doc = match realm.document::<ShareDelivery>(&key).await {
                Ok(d) => d,
                Err(_) => continue,
            };
            let payload = ShareDelivery {
                encrypted_share: share_bytes[idx].clone(),
                sender_user_id: my_uid,
                created_at_millis: now,
                label: labels[idx].clone(),
            };
            if doc.update(move |d| *d = payload).await.is_ok() {
                delivered_to.push(labels[idx].clone());
            }
        }
    }

    Ok(SetupOutcome {
        shares_hex,
        delivered_to,
    })
}

/// Build a `UserId` → `RealmId` map covering every DM realm the user
/// currently belongs to. Returns empty if the network has no DM realms
/// or their peer-keys directory has not synced yet.
async fn dm_realm_map(
    network: &Arc<IndrasNetwork>,
) -> std::collections::BTreeMap<[u8; 32], indras_network::RealmId> {
    use std::collections::BTreeMap;

    let my_uid = network.node().pq_identity().user_id();
    let mut map = BTreeMap::new();

    for realm_id in network.conversation_realms() {
        if network.dm_peer_for_realm(&realm_id).is_none() {
            continue;
        }
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };
        let Ok(doc) = realm.document::<PeerKeyDirectory>("peer-keys").await else {
            continue;
        };
        let data = doc.read().await;
        for (uid, _ek) in data.peers_with_kem() {
            if uid != my_uid {
                // In a DM realm there's exactly one non-self peer, so
                // first-writer wins here is correct.
                map.entry(uid).or_insert(realm_id);
            }
        }
    }

    map
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
    /// Hex-encoded `UserId` for the peer — used to route in-band share
    /// delivery into the correct DM realm.
    pub user_id_hex: String,
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
            let uid_hex = hex::encode(uid);
            let label = name.unwrap_or_else(|| format!("Peer {}", &uid_hex[..8]));
            AvailableSteward {
                label,
                ek_hex: hex::encode(ek.to_bytes()),
                user_id_hex: uid_hex,
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

/// Walk every DM realm, probe each non-self peer's share-delivery doc,
/// materialize any hits into `<data_dir>/steward_holdings.json`, and
/// return the refreshed cache.
///
/// Safe to call at any time from the UI — idempotent, read-mostly,
/// writes the file only when content changes.
pub async fn refresh_held_backups(network: Arc<IndrasNetwork>) -> StewardHoldings {
    let data_dir = crate::state::default_data_dir();
    let my_uid = network.node().pq_identity().user_id();
    let mut holdings = StewardHoldings::default();

    for realm_id in network.conversation_realms() {
        if network.dm_peer_for_realm(&realm_id).is_none() {
            continue;
        }
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };
        let candidate_uids: Vec<[u8; 32]> = match realm
            .document::<PeerKeyDirectory>("peer-keys")
            .await
        {
            Ok(doc) => doc
                .read()
                .await
                .peers_with_kem()
                .into_iter()
                .map(|(uid, _)| uid)
                .filter(|uid| *uid != my_uid)
                .collect(),
            Err(_) => continue,
        };

        for sender_uid in candidate_uids {
            let key = share_delivery_doc_key(&sender_uid);
            let Ok(doc) = realm.document::<ShareDelivery>(&key).await else {
                continue;
            };
            let snap = doc.read().await.clone();
            // Skip the zero/default state produced by opening a doc
            // that was never written.
            if snap.created_at_millis == 0 || snap.encrypted_share.is_empty() {
                continue;
            }
            let sender_hex = hex::encode(sender_uid);
            holdings.by_sender.insert(
                sender_hex.clone(),
                HeldBackup {
                    sender_user_id_hex: sender_hex,
                    label: snap.label.clone(),
                    created_at_millis: snap.created_at_millis,
                },
            );
        }
    }

    // Best-effort persistence — failure doesn't affect the return.
    let _ = holdings.save(&data_dir);
    holdings
}

/// Load the previously-persisted holdings cache, or an empty cache if
/// the file doesn't exist. Cheap; safe to call during app startup to
/// seed the status-bar badge before the first network scan completes.
pub fn load_held_backups() -> StewardHoldings {
    let data_dir = crate::state::default_data_dir();
    StewardHoldings::load(&data_dir).unwrap_or_default()
}

// ──────────────────────────────────────────────────────────────────
// Steward enrollment — invitation / response handshake (A.2)
// ──────────────────────────────────────────────────────────────────

/// Sender-side view of one peer's enrollment state. Drives the
/// per-row badge in the Backup-plan overlay.
#[derive(Debug, Clone)]
pub struct OutgoingEnrollment {
    /// The peer's `UserId` as hex.
    pub peer_user_id_hex: String,
    /// Peer display name resolved through the DM profile mirror,
    /// falling back to `Peer {hex8}` when unresolvable.
    pub peer_label: String,
    /// Plain-language badge state for the UI.
    pub status: EnrollmentStatus,
    /// Peer's ML-KEM-768 encapsulation key as hex, *if* the peer has
    /// accepted and supplied one. Empty otherwise. The Backup-plan
    /// overlay uses this on quorum to wrap the per-steward share.
    pub accepted_kem_ek_hex: String,
}

/// Steward-side view of one pending invitation from a DM peer.
#[derive(Debug, Clone)]
pub struct IncomingInvitation {
    /// Sender's `UserId` hex. Used as the reply routing key.
    pub from_user_id_hex: String,
    /// Sender's display name at invitation time.
    pub from_display_name: String,
    /// Plain-language responsibility copy to render verbatim in the
    /// approve/decline dialog.
    pub responsibility_text: String,
    /// K-of-N parameters as the sender chose them.
    pub threshold_k: u8,
    pub total_n: u8,
    /// Wall-clock millis when the invitation was issued.
    pub issued_at_millis: i64,
    /// `true` once the steward has responded (whether accept or
    /// decline). Keeps the inbox de-duped.
    pub already_responded: bool,
    /// Last-response `accepted` flag. Meaningful only when
    /// `already_responded` is true. Drives "you're a backup friend
    /// for X" vs "you declined" views.
    pub last_response_accepted: bool,
}

/// Publish a steward invitation to the DM peer identified by
/// `peer_user_id_hex`. Fails when the peer isn't a current DM peer
/// with a known KEM key. Re-invitation bumps the timestamp so
/// last-writer-wins merge supersedes any prior state.
pub async fn invite_steward(
    network: Arc<IndrasNetwork>,
    peer_user_id_hex: &str,
    threshold_k: u8,
    total_n: u8,
    responsibility_text: Option<String>,
) -> Result<(), String> {
    let peer_uid = decode_user_id(peer_user_id_hex)
        .ok_or_else(|| "Invalid peer id".to_string())?;
    let realm_id = dm_realm_for_uid(&network, &peer_uid).await.ok_or_else(|| {
        "That friend isn't a direct-message peer yet — add them as a contact first.".to_string()
    })?;
    let realm = network
        .get_realm_by_id(&realm_id)
        .ok_or_else(|| "Couldn't open the direct-message realm.".to_string())?;
    let key = invite_doc_key(&network.node().pq_identity().user_id());
    let doc = realm
        .document::<StewardInvitation>(&key)
        .await
        .map_err(|e| format!("Couldn't open invitation doc: {}", e))?;

    let my_uid = network.node().pq_identity().user_id();
    let my_name = network.display_name().unwrap_or_default();
    let payload = StewardInvitation {
        from_user_id: my_uid,
        from_display_name: my_name,
        responsibility_text: responsibility_text
            .unwrap_or_else(|| DEFAULT_RESPONSIBILITY.to_string()),
        threshold_k,
        total_n,
        issued_at_millis: chrono::Utc::now().timestamp_millis(),
        withdrawn: false,
    };
    doc.update(move |d| *d = payload)
        .await
        .map_err(|e| format!("Couldn't publish invitation: {}", e))?;
    Ok(())
}

/// Mark a previously-issued invitation as withdrawn so the steward's
/// UI shows it vanishing and any prior acceptance is superseded for
/// quorum purposes.
pub async fn revoke_invitation(
    network: Arc<IndrasNetwork>,
    peer_user_id_hex: &str,
) -> Result<(), String> {
    let peer_uid = decode_user_id(peer_user_id_hex)
        .ok_or_else(|| "Invalid peer id".to_string())?;
    let realm_id = dm_realm_for_uid(&network, &peer_uid)
        .await
        .ok_or_else(|| "That friend isn't a direct-message peer.".to_string())?;
    let realm = network
        .get_realm_by_id(&realm_id)
        .ok_or_else(|| "Couldn't open the direct-message realm.".to_string())?;
    let key = invite_doc_key(&network.node().pq_identity().user_id());
    let doc = realm
        .document::<StewardInvitation>(&key)
        .await
        .map_err(|e| format!("Couldn't open invitation doc: {}", e))?;

    let my_uid = network.node().pq_identity().user_id();
    let my_name = network.display_name().unwrap_or_default();
    let payload = StewardInvitation {
        from_user_id: my_uid,
        from_display_name: my_name,
        responsibility_text: String::new(),
        threshold_k: 0,
        total_n: 0,
        issued_at_millis: chrono::Utc::now().timestamp_millis(),
        withdrawn: true,
    };
    doc.update(move |d| *d = payload)
        .await
        .map_err(|e| format!("Couldn't withdraw invitation: {}", e))?;
    Ok(())
}

/// Walk every DM realm the user belongs to and report the current
/// enrollment state for each non-self peer that has a KEM key on
/// file. Used by the Backup-plan overlay to render the peer list
/// with per-row status badges.
pub async fn list_outgoing_enrollments(
    network: Arc<IndrasNetwork>,
) -> Vec<OutgoingEnrollment> {
    let my_uid = network.node().pq_identity().user_id();
    let my_invite_key = invite_doc_key(&my_uid);
    let my_response_key = response_doc_key(&my_uid);

    let mut out = Vec::new();
    for realm_id in network.conversation_realms() {
        if network.dm_peer_for_realm(&realm_id).is_none() {
            continue;
        }
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };

        // Resolve peer label via profile mirror (DM-only path).
        let dm_peer_name = match network.dm_peer_for_realm(&realm_id) {
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

        let Ok(key_dir) = realm.document::<PeerKeyDirectory>("peer-keys").await else {
            continue;
        };
        let peer_uids: Vec<[u8; 32]> = {
            let data = key_dir.read().await;
            data.peers_with_kem()
                .into_iter()
                .map(|(uid, _)| uid)
                .filter(|uid| *uid != my_uid)
                .collect()
        };
        if peer_uids.is_empty() {
            continue;
        }

        // Invitation we've written in this realm (keyed by our UID).
        let invite_doc = realm
            .document::<StewardInvitation>(&my_invite_key)
            .await
            .ok();
        let invitation = match invite_doc.as_ref() {
            Some(d) => {
                let snap = d.read().await.clone();
                if snap.issued_at_millis == 0 {
                    None
                } else {
                    Some(snap)
                }
            }
            None => None,
        };

        // Peer's response (keyed by our UID too — steward writes under
        // the sender's UID).
        let response_doc = realm
            .document::<StewardResponse>(&my_response_key)
            .await
            .ok();
        let response = match response_doc.as_ref() {
            Some(d) => {
                let snap = d.read().await.clone();
                if snap.responded_at_millis == 0 {
                    None
                } else {
                    Some(snap)
                }
            }
            None => None,
        };

        let status = EnrollmentStatus::derive(invitation.as_ref(), response.as_ref());
        let accepted_kem_ek_hex = match &response {
            Some(r) if r.accepted && !r.kem_ek_bytes.is_empty() => hex::encode(&r.kem_ek_bytes),
            _ => String::new(),
        };

        for uid in peer_uids {
            let uid_hex = hex::encode(uid);
            let peer_label = dm_peer_name
                .clone()
                .unwrap_or_else(|| format!("Peer {}", &uid_hex[..8]));
            out.push(OutgoingEnrollment {
                peer_user_id_hex: uid_hex,
                peer_label,
                status: status.clone(),
                accepted_kem_ek_hex: accepted_kem_ek_hex.clone(),
            });
        }
    }
    out
}

/// Walk every DM realm and collect invitations *received* from each
/// peer — the input to the Steward inbox overlay. Every entry
/// corresponds to one peer's `_steward_invite:{peer_uid}` doc.
pub async fn list_incoming_invitations(
    network: Arc<IndrasNetwork>,
) -> Vec<IncomingInvitation> {
    let my_uid = network.node().pq_identity().user_id();

    let mut out = Vec::new();
    for realm_id in network.conversation_realms() {
        if network.dm_peer_for_realm(&realm_id).is_none() {
            continue;
        }
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };

        let Ok(key_dir) = realm.document::<PeerKeyDirectory>("peer-keys").await else {
            continue;
        };
        let peer_uids: Vec<[u8; 32]> = {
            let data = key_dir.read().await;
            data.peers_with_kem()
                .into_iter()
                .map(|(uid, _)| uid)
                .filter(|uid| *uid != my_uid)
                .collect()
        };

        for peer_uid in peer_uids {
            let invite_key = invite_doc_key(&peer_uid);
            let response_key = response_doc_key(&peer_uid);

            let invite_snap = match realm.document::<StewardInvitation>(&invite_key).await {
                Ok(d) => {
                    let s = d.read().await.clone();
                    if s.issued_at_millis == 0 || s.withdrawn {
                        continue;
                    }
                    s
                }
                Err(_) => continue,
            };

            let (already_responded, last_response_accepted) = match realm
                .document::<StewardResponse>(&response_key)
                .await
            {
                Ok(d) => {
                    let s = d.read().await.clone();
                    if s.responded_at_millis == 0
                        || s.responded_at_millis < invite_snap.issued_at_millis
                    {
                        (false, false)
                    } else {
                        (true, s.accepted)
                    }
                }
                Err(_) => (false, false),
            };

            out.push(IncomingInvitation {
                from_user_id_hex: hex::encode(peer_uid),
                from_display_name: invite_snap.from_display_name,
                responsibility_text: invite_snap.responsibility_text,
                threshold_k: invite_snap.threshold_k,
                total_n: invite_snap.total_n,
                issued_at_millis: invite_snap.issued_at_millis,
                already_responded,
                last_response_accepted,
            });
        }
    }
    out
}

/// Steward-side: reply to an invitation. On accept, publish the
/// current device's fresh ML-KEM encapsulation key so the sender
/// can wrap a share for this device without needing to hit the
/// peer-key directory separately.
pub async fn respond_to_invitation(
    network: Arc<IndrasNetwork>,
    from_user_id_hex: &str,
    accept: bool,
) -> Result<(), String> {
    let from_uid = decode_user_id(from_user_id_hex)
        .ok_or_else(|| "Invalid sender id".to_string())?;
    let realm_id = dm_realm_for_uid(&network, &from_uid)
        .await
        .ok_or_else(|| "That sender isn't a direct-message peer.".to_string())?;
    let realm = network
        .get_realm_by_id(&realm_id)
        .ok_or_else(|| "Couldn't open the direct-message realm.".to_string())?;

    let key = response_doc_key(&from_uid);
    let doc = realm
        .document::<StewardResponse>(&key)
        .await
        .map_err(|e| format!("Couldn't open response doc: {}", e))?;

    let my_uid = network.node().pq_identity().user_id();
    let my_kem_ek = network.node().pq_kem_keypair().encapsulation_key_bytes();
    let my_vk = network.node().pq_identity().verifying_key_bytes();
    let payload = StewardResponse {
        steward_user_id: my_uid,
        accepted: accept,
        responded_at_millis: chrono::Utc::now().timestamp_millis(),
        kem_ek_bytes: if accept { my_kem_ek } else { Vec::new() },
        dsa_vk_bytes: my_vk,
    };
    doc.update(move |d| *d = payload)
        .await
        .map_err(|e| format!("Couldn't publish response: {}", e))?;
    Ok(())
}

fn decode_user_id(hex_input: &str) -> Option<[u8; 32]> {
    let bytes = hex::decode(hex_input.trim()).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

/// Find the DM `RealmId` we share with the peer identified by
/// `target_uid`, if one exists and has the target's KEM entry. This
/// is a narrower view of `dm_realm_map` for a single lookup.
async fn dm_realm_for_uid(
    network: &Arc<IndrasNetwork>,
    target_uid: &[u8; 32],
) -> Option<indras_network::RealmId> {
    for realm_id in network.conversation_realms() {
        if network.dm_peer_for_realm(&realm_id).is_none() {
            continue;
        }
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };
        let Ok(doc) = realm.document::<PeerKeyDirectory>("peer-keys").await else {
            continue;
        };
        let hit = {
            let data = doc.read().await;
            data.peers_with_kem()
                .into_iter()
                .any(|(uid, _)| uid == *target_uid)
        };
        if hit {
            return Some(realm_id);
        }
    }
    None
}
