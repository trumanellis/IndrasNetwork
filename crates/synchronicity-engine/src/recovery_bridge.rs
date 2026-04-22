//! Bridge for the Plan-A steward enrollment + recovery flows.
//!
//! **Setup side**: the Backup-plan overlay calls `invite_steward`
//! per peer, then `finalize_steward_split` when the accepted set
//! reaches the quorum K. The split is performed against the cached
//! encryption subkey (populated at sign-in) so the user never
//! retypes their story for a backup.
//!
//! **Steward side**: the inbox overlay calls `list_incoming_*` and
//! `respond_to_invitation` / `approve_recovery_request`. On
//! approval, the steward decrypts their held share with their own
//! KEM keypair, re-wraps it to the recovering device's fresh KEM
//! ek, and publishes a `ShareRelease` doc in the same DM realm.
//!
//! **Recovery side**: the Recovery overlay calls `initiate_recovery`
//! on the selected stewards, polls `_share_release:*` docs via
//! `poll_recovery_releases`, and once K land calls
//! `assemble_and_authenticate` to rebuild the subkey and re-unlock
//! the on-disk keystore.
//!
//! All share material stays wrapped inside CRDT docs that live in
//! DM realms — the user never sees hex. The crypto primitives (ML-
//! KEM-768 + Shamir) are unchanged from Phase 1; Plan A reshapes
//! the UX, Plan B will reshape the *identity* being protected.

use std::sync::Arc;

use indras_crypto::pq_kem::PQEncapsulationKey;
use indras_crypto::story_template::PassStory;
use indras_network::IndrasNetwork;
use indras_node::StoryKeystore;
use indras_sync_engine::peer_key_directory::PeerKeyDirectory;
use indras_sync_engine::account_root_cache;
use indras_sync_engine::account_root_envelope;
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

// ──────────────────────────────────────────────────────────────────
// Quorum-driven share distribution (A.5)
// ──────────────────────────────────────────────────────────────────

/// One accepted steward's info needed to produce an encrypted share.
#[derive(Clone, Debug)]
pub struct AcceptedSteward {
    pub peer_user_id_hex: String,
    pub peer_label: String,
    pub accepted_kem_ek_hex: String,
}

/// Split the cached encryption subkey into K-of-N shares and publish
/// one to each accepted steward's DM realm. Called automatically by
/// the Backup-plan overlay whenever the accepted-peer set reaches or
/// exceeds K. Re-running supersedes earlier splits via last-writer-
/// wins on the `_steward_share:*` doc.
///
/// Returns the number of stewards that received a share this run.
/// Stewards whose DM realm isn't resolvable at call time are skipped
/// silently — they'll pick up the next re-issue.
pub async fn finalize_steward_split(
    network: Arc<IndrasNetwork>,
    accepted: Vec<AcceptedSteward>,
    threshold_k: u8,
) -> Result<(usize, [u8; 32]), String> {
    if threshold_k < 2 {
        return Err("Need at least 2 friends to agree.".to_string());
    }
    if accepted.len() < threshold_k as usize {
        return Err(format!(
            "Only {} friends accepted — need {}.",
            accepted.len(),
            threshold_k
        ));
    }

    let data_dir = crate::state::default_data_dir();

    // Prefer the Plan-B pending AccountRoot: seal it under a fresh
    // 32-byte wrapping key, publish the envelope to the home realm,
    // and Shamir-split the wrapping key instead of the legacy
    // story-derived subkey. Falls back to the subkey cache for
    // accounts created before B.4.
    let subkey: [u8; 32] = match account_root_cache::load_pending_root(&data_dir) {
        Some(root) => {
            use rand::RngCore;
            let mut wrapping_key = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut wrapping_key);
            let version = chrono::Utc::now().timestamp_millis() as u64;
            let envelope = account_root_envelope::seal_account_root(&root, &wrapping_key, version)
                .map_err(|e| format!("Seal root: {e}"))?;
            let home = network
                .home_realm()
                .await
                .map_err(|e| format!("home realm unavailable: {e}"))?;
            let env_doc = home
                .document::<account_root_envelope::AccountRootEnvelope>(
                    account_root_envelope::ACCOUNT_ROOT_ENVELOPE_DOC_KEY,
                )
                .await
                .map_err(|e| format!("open envelope doc: {e}"))?;
            env_doc
                .update(move |d| *d = envelope)
                .await
                .map_err(|e| format!("publish envelope: {e}"))?;
            wrapping_key
        }
        None => load_subkey_cache(&data_dir).ok_or_else(|| {
            "No backup key on this device yet. Sign in with your story once to cache it."
                .to_string()
        })?,
    };

    // Build stewards + per-steward UIDs for routing.
    let mut stewards = Vec::with_capacity(accepted.len());
    let mut uids = Vec::with_capacity(accepted.len());
    for acc in &accepted {
        let ek_bytes = hex::decode(acc.accepted_kem_ek_hex.trim()).map_err(|e| {
            format!("Steward `{}`: backup-code hex decode failed — {}", acc.peer_label, e)
        })?;
        let ek = PQEncapsulationKey::from_bytes(&ek_bytes).map_err(|e| {
            format!("Steward `{}`: invalid backup key — {}", acc.peer_label, e)
        })?;
        let uid = decode_user_id(&acc.peer_user_id_hex)
            .ok_or_else(|| format!("Steward `{}`: invalid user id", acc.peer_label))?;
        stewards.push((StewardId::new(acc.peer_label.as_bytes().to_vec()), ek));
        uids.push(uid);
    }

    // Split + persist the manifest. `prepare_recovery` is pure CPU
    // work so spawn-blocking keeps the UI async runtime happy.
    let (prepared, data_dir_owned) = {
        let data_dir_owned = data_dir.clone();
        let stewards_owned = stewards.clone();
        let prepared = tokio::task::spawn_blocking(move || -> Result<_, String> {
            let p = steward_recovery::prepare_recovery(
                &subkey,
                &stewards_owned,
                threshold_k,
                chrono::Utc::now().timestamp_millis() as u64,
            )
            .map_err(|e| format!("{}", e))?;
            steward_recovery::save_manifest(&data_dir_owned, &p.manifest)
                .map_err(|e| format!("{}", e))?;
            Ok(p)
        })
        .await
        .map_err(|e| format!("task join error: {}", e))??;
        (prepared, data_dir)
    };
    let _ = data_dir_owned; // silence unused when all code-paths drop it early

    // Publish each share to the matching DM realm. Failures per peer
    // are swallowed so a single unreachable steward doesn't block the
    // whole split.
    let my_uid = network.node().pq_identity().user_id();
    let now = chrono::Utc::now().timestamp_millis();
    let key = share_delivery_doc_key(&my_uid);
    let mut delivered = 0usize;
    for (idx, enc_share) in prepared.encrypted_shares.iter().enumerate() {
        let Some(realm_id) = dm_realm_for_uid(&network, &uids[idx]).await else {
            continue;
        };
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };
        let Ok(doc) = realm.document::<ShareDelivery>(&key).await else {
            continue;
        };
        let bytes = match enc_share.to_bytes() {
            Ok(b) => b,
            Err(_) => continue,
        };
        let payload = ShareDelivery {
            encrypted_share: bytes,
            sender_user_id: my_uid,
            created_at_millis: now,
            label: accepted[idx].peer_label.clone(),
        };
        if doc.update(move |d| *d = payload).await.is_ok() {
            delivered += 1;
        }
    }

    // Once the quorum has received their share we can drop the
    // pending root cache — stewards now collectively hold the
    // wrapping key that unseals the envelope.
    if delivered >= threshold_k as usize {
        account_root_cache::clear_pending_root(&data_dir_owned);
    }

    Ok((delivered, subkey))
}

// ──────────────────────────────────────────────────────────────────
// Recovery request + release protocol (A.6)
// ──────────────────────────────────────────────────────────────────

/// Steward-side view of an incoming recovery request.
#[derive(Debug, Clone)]
pub struct IncomingRecoveryRequest {
    /// New device's `UserId` as hex.
    pub new_device_uid_hex: String,
    /// Display name the new device presented.
    pub new_device_display_name: String,
    /// `UserId` of the DM peer we share this realm with. In a DM
    /// realm the new device's UID equals this — same value, just
    /// echoed through two paths for UI convenience.
    pub dm_peer_uid_hex: String,
    /// Wall-clock millis when the request was issued.
    pub issued_at_millis: i64,
    /// `true` if this steward already responded with a release.
    pub already_released: bool,
}

/// New-device view of progress collecting releases.
#[derive(Debug, Clone, Default)]
pub struct RecoveryProgress {
    /// Hex-encoded steward UIDs that have released a share.
    pub released_by: Vec<String>,
    /// Hex-encoded source-account UIDs claimed across the released
    /// shares. Normal recovery has exactly one; more than one
    /// indicates stewards are releasing for different accounts.
    pub source_accounts: Vec<String>,
}

impl RecoveryProgress {
    /// Count of stewards who've released so far.
    pub fn count(&self) -> usize {
        self.released_by.len()
    }
}

/// Publish a recovery request into the DM realm shared with each
/// selected steward. The request carries the new device's display
/// name and fresh KEM ek so the steward can re-wrap their share
/// directly.
pub async fn initiate_recovery(
    network: Arc<IndrasNetwork>,
    selected_steward_uids_hex: Vec<String>,
) -> Result<usize, String> {
    let my_uid = network.node().pq_identity().user_id();
    let my_kem_ek = network.node().pq_kem_keypair().encapsulation_key_bytes();
    let my_vk = network.node().pq_identity().verifying_key_bytes();
    let my_name = network.display_name().unwrap_or_default();
    let key = recovery_protocol_key_request(&my_uid);
    let now = chrono::Utc::now().timestamp_millis();

    let mut published = 0usize;
    for steward_hex in selected_steward_uids_hex {
        let Some(steward_uid) = decode_user_id(&steward_hex) else {
            continue;
        };
        let Some(realm_id) = dm_realm_for_uid(&network, &steward_uid).await else {
            continue;
        };
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };
        let Ok(doc) = realm
            .document::<indras_sync_engine::recovery_protocol::RecoveryRequest>(&key)
            .await
        else {
            continue;
        };
        let payload = indras_sync_engine::recovery_protocol::RecoveryRequest {
            new_device_uid: my_uid,
            new_device_display_name: my_name.clone(),
            new_device_kem_ek: my_kem_ek.clone(),
            new_device_vk: my_vk.clone(),
            issued_at_millis: now,
            withdrawn: false,
        };
        if doc.update(move |d| *d = payload).await.is_ok() {
            published += 1;
        }
    }

    if published == 0 {
        return Err(
            "None of the friends you selected are reachable as direct-message peers."
                .to_string(),
        );
    }
    Ok(published)
}

/// Withdraw the current recovery request so stewards stop surfacing
/// it. Best-effort across every DM realm the request was pushed to.
pub async fn withdraw_recovery_request(network: Arc<IndrasNetwork>) -> Result<(), String> {
    let my_uid = network.node().pq_identity().user_id();
    let key = recovery_protocol_key_request(&my_uid);
    let now = chrono::Utc::now().timestamp_millis();

    for realm_id in network.conversation_realms() {
        if network.dm_peer_for_realm(&realm_id).is_none() {
            continue;
        }
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };
        let Ok(doc) = realm
            .document::<indras_sync_engine::recovery_protocol::RecoveryRequest>(&key)
            .await
        else {
            continue;
        };
        let payload = indras_sync_engine::recovery_protocol::RecoveryRequest {
            new_device_uid: my_uid,
            new_device_display_name: String::new(),
            new_device_kem_ek: Vec::new(),
            new_device_vk: Vec::new(),
            issued_at_millis: now,
            withdrawn: true,
        };
        let _ = doc.update(move |d| *d = payload).await;
    }
    Ok(())
}

/// Steward-side: walk DM realms and collect live recovery requests.
pub async fn list_incoming_recovery_requests(
    network: Arc<IndrasNetwork>,
) -> Vec<IncomingRecoveryRequest> {
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
            let req_key = recovery_protocol_key_request(&peer_uid);
            let rel_key = recovery_protocol_key_release(&peer_uid);

            let req_snap = match realm
                .document::<indras_sync_engine::recovery_protocol::RecoveryRequest>(&req_key)
                .await
            {
                Ok(d) => {
                    let s = d.read().await.clone();
                    if s.issued_at_millis == 0 || s.withdrawn {
                        continue;
                    }
                    s
                }
                Err(_) => continue,
            };

            let already_released = match realm
                .document::<indras_sync_engine::recovery_protocol::ShareRelease>(&rel_key)
                .await
            {
                Ok(d) => {
                    let s = d.read().await.clone();
                    s.approved_at_millis >= req_snap.issued_at_millis
                        && !s.encrypted_share_bytes.is_empty()
                }
                Err(_) => false,
            };

            out.push(IncomingRecoveryRequest {
                new_device_uid_hex: hex::encode(peer_uid),
                new_device_display_name: req_snap.new_device_display_name,
                dm_peer_uid_hex: hex::encode(peer_uid),
                issued_at_millis: req_snap.issued_at_millis,
                already_released,
            });
        }
    }
    out
}

/// Steward-side: approve a recovery request.
///
/// Loads this steward's held share for `source_account_uid_hex`,
/// decrypts with the steward's own KEM keypair, re-encrypts to the
/// new device's KEM ek, and publishes a `ShareRelease` doc into the
/// DM realm shared with the new device.
pub async fn approve_recovery_request(
    network: Arc<IndrasNetwork>,
    requester_uid_hex: &str,
    source_account_uid_hex: &str,
) -> Result<(), String> {
    let requester_uid = decode_user_id(requester_uid_hex)
        .ok_or_else(|| "Invalid requester id".to_string())?;
    let source_uid = decode_user_id(source_account_uid_hex)
        .ok_or_else(|| "Invalid source-account id".to_string())?;

    // 1. Locate the held share (published by the source account into
    //    the DM realm this steward shares with them).
    let source_realm_id = dm_realm_for_uid(&network, &source_uid)
        .await
        .ok_or_else(|| "Source-account friend isn't a direct-message peer.".to_string())?;
    let source_realm = network
        .get_realm_by_id(&source_realm_id)
        .ok_or_else(|| "Couldn't open source-account realm.".to_string())?;
    let share_key = share_delivery_doc_key(&source_uid);
    let share_delivery = {
        let doc = source_realm
            .document::<ShareDelivery>(&share_key)
            .await
            .map_err(|e| format!("Couldn't open share doc: {}", e))?;
        doc.read().await.clone()
    };
    if share_delivery.created_at_millis == 0 || share_delivery.encrypted_share.is_empty() {
        return Err("You don't have a backup piece from that friend.".to_string());
    }

    // 2. Decrypt with this steward's own KEM keypair.
    let encrypted = indras_crypto::steward_share::EncryptedStewardShare::from_bytes(
        &share_delivery.encrypted_share,
    )
    .map_err(|e| format!("Couldn't read the backup piece: {}", e))?;
    let my_kp = {
        let dk = network.node().pq_kem_keypair().decapsulation_key_bytes();
        let ek = network.node().pq_kem_keypair().encapsulation_key_bytes();
        indras_crypto::pq_kem::PQKemKeyPair::from_keypair_bytes(dk.as_slice(), &ek)
            .map_err(|e| format!("Couldn't rebuild KEM keypair: {}", e))?
    };
    let share = encrypted
        .decrypt(&my_kp)
        .map_err(|e| format!("Couldn't decrypt the backup piece: {}", e))?;
    let threshold = encrypted.threshold;
    let secret_version = encrypted.secret_version;

    // 3. Re-wrap against the new device's KEM ek (pulled from the
    //    request doc in the requester's DM realm).
    let req_realm_id = dm_realm_for_uid(&network, &requester_uid)
        .await
        .ok_or_else(|| "That requester isn't a direct-message peer.".to_string())?;
    let req_realm = network
        .get_realm_by_id(&req_realm_id)
        .ok_or_else(|| "Couldn't open requester realm.".to_string())?;
    let req_key = recovery_protocol_key_request(&requester_uid);
    let request_snap = {
        let doc = req_realm
            .document::<indras_sync_engine::recovery_protocol::RecoveryRequest>(&req_key)
            .await
            .map_err(|e| format!("Couldn't open request doc: {}", e))?;
        doc.read().await.clone()
    };
    if request_snap.issued_at_millis == 0 || request_snap.withdrawn {
        return Err("That recovery request isn't active anymore.".to_string());
    }
    let new_device_ek =
        indras_crypto::pq_kem::PQEncapsulationKey::from_bytes(&request_snap.new_device_kem_ek)
            .map_err(|e| format!("New device's backup key is malformed: {}", e))?;
    let new_encrypted = indras_crypto::steward_share::encrypt_share_for_steward(
        &share,
        threshold,
        secret_version,
        &new_device_ek,
    )
    .map_err(|e| format!("Couldn't seal the piece for the new device: {}", e))?;
    let new_bytes = new_encrypted
        .to_bytes()
        .map_err(|e| format!("Couldn't serialize sealed piece: {}", e))?;

    // 4. Publish `_share_release:{requester_uid}` into the requester
    //    realm.
    let rel_key = recovery_protocol_key_release(&requester_uid);
    let rel_doc = req_realm
        .document::<indras_sync_engine::recovery_protocol::ShareRelease>(&rel_key)
        .await
        .map_err(|e| format!("Couldn't open release doc: {}", e))?;
    let payload = indras_sync_engine::recovery_protocol::ShareRelease {
        steward_uid: network.node().pq_identity().user_id(),
        source_account_uid: source_uid,
        encrypted_share_bytes: new_bytes,
        approved_at_millis: chrono::Utc::now().timestamp_millis(),
    };
    rel_doc
        .update(move |d| *d = payload)
        .await
        .map_err(|e| format!("Couldn't publish release: {}", e))?;

    Ok(())
}

/// New-device side: walk every DM realm and collect `_share_release`
/// entries keyed by our own UID.
pub async fn poll_recovery_releases(network: Arc<IndrasNetwork>) -> RecoveryProgress {
    use std::collections::BTreeSet;
    let my_uid = network.node().pq_identity().user_id();
    let key = recovery_protocol_key_release(&my_uid);
    let mut released_by = BTreeSet::new();
    let mut source_accounts = BTreeSet::new();

    for realm_id in network.conversation_realms() {
        let Some(peer_mid) = network.dm_peer_for_realm(&realm_id) else {
            continue;
        };
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };
        let Ok(doc) = realm
            .document::<indras_sync_engine::recovery_protocol::ShareRelease>(&key)
            .await
        else {
            continue;
        };
        let snap = doc.read().await.clone();
        if snap.approved_at_millis == 0 || snap.encrypted_share_bytes.is_empty() {
            continue;
        }
        // Plan-C gate: require the releasing steward's device to
        // carry a non-revoked, root-signed cert in their own home-
        // realm DeviceRoster. Fail closed — a missing roster or a
        // revoked cert skips this release; the poll loop will
        // retry once more state lands.
        if !indras_sync_engine::peer_verification::gate_peer_via_home_roster(
            &network,
            &realm_id,
            &peer_mid,
            &snap.steward_uid,
        )
        .await
        {
            continue;
        }
        released_by.insert(hex::encode(snap.steward_uid));
        source_accounts.insert(hex::encode(snap.source_account_uid));
    }

    RecoveryProgress {
        released_by: released_by.into_iter().collect(),
        source_accounts: source_accounts.into_iter().collect(),
    }
}

/// New-device side: once `poll_recovery_releases` shows K releases,
/// decrypt each with the new device's own KEM keypair, recombine
/// the subkey, and re-auth the local keystore. On success the
/// subkey is also cached so future setups don't require story
/// re-entry.
pub async fn assemble_and_authenticate(
    network: Arc<IndrasNetwork>,
    threshold_k: u8,
) -> Result<[u8; 32], String> {
    if threshold_k < 2 {
        return Err("Need at least 2 pieces.".to_string());
    }

    let my_uid = network.node().pq_identity().user_id();
    let key = recovery_protocol_key_release(&my_uid);

    // Collect EncryptedStewardShare payloads from every DM realm.
    let mut encrypted_shares = Vec::new();
    for realm_id in network.conversation_realms() {
        let Some(peer_mid) = network.dm_peer_for_realm(&realm_id) else {
            continue;
        };
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };
        let Ok(doc) = realm
            .document::<indras_sync_engine::recovery_protocol::ShareRelease>(&key)
            .await
        else {
            continue;
        };
        let snap = doc.read().await.clone();
        if snap.approved_at_millis == 0 || snap.encrypted_share_bytes.is_empty() {
            continue;
        }
        // Gate the release on the steward's own device roster —
        // refuse to decrypt shares from revoked or unknown devices.
        if !indras_sync_engine::peer_verification::gate_peer_via_home_roster(
            &network,
            &realm_id,
            &peer_mid,
            &snap.steward_uid,
        )
        .await
        {
            continue;
        }
        encrypted_shares.push(snap.encrypted_share_bytes);
    }

    if encrypted_shares.len() < threshold_k as usize {
        return Err(format!(
            "Only {} friends have released a piece — need {}.",
            encrypted_shares.len(),
            threshold_k
        ));
    }

    let my_kp = {
        let dk = network.node().pq_kem_keypair().decapsulation_key_bytes();
        let ek = network.node().pq_kem_keypair().encapsulation_key_bytes();
        indras_crypto::pq_kem::PQKemKeyPair::from_keypair_bytes(dk.as_slice(), &ek)
            .map_err(|e| format!("Couldn't rebuild KEM keypair: {}", e))?
    };

    let mut shamir_shares = Vec::with_capacity(encrypted_shares.len());
    for bytes in &encrypted_shares {
        let encrypted = indras_crypto::steward_share::EncryptedStewardShare::from_bytes(bytes)
            .map_err(|e| format!("Couldn't read a piece: {}", e))?;
        let share = encrypted
            .decrypt(&my_kp)
            .map_err(|e| format!("Couldn't decrypt a piece: {}", e))?;
        shamir_shares.push(share);
    }

    let data_dir = crate::state::default_data_dir();
    let subkey = steward_recovery::recover_encryption_subkey(&shamir_shares, threshold_k)
        .map_err(|e| format!("Couldn't reassemble the backup: {}", e))?;

    // Plan-B path: if the account published an AccountRoot envelope
    // into the home realm, the reassembled subkey is the wrapping
    // key. Unseal the root, stamp a fresh DeviceCertificate for this
    // device, and publish it into the DeviceRoster. The root is
    // dropped immediately after signing.
    if let Ok(home) = network.home_realm().await {
        if let Ok(env_doc) = home
            .document::<account_root_envelope::AccountRootEnvelope>(
                account_root_envelope::ACCOUNT_ROOT_ENVELOPE_DOC_KEY,
            )
            .await
        {
            let env = env_doc.read().await.clone();
            if env.version > 0 && !env.encrypted_sk.is_empty() {
                match account_root_envelope::unseal_account_root(&env, &subkey) {
                    Ok(root) => {
                        let device_vk = network.node().pq_identity().verifying_key_bytes();
                        let device_name = network
                            .display_name()
                            .filter(|n| !n.trim().is_empty())
                            .map(|n| format!("{}'s recovered device", n.trim()))
                            .unwrap_or_else(|| "Recovered device".to_string());
                        let now = chrono::Utc::now().timestamp_millis();
                        let cert = indras_crypto::device_cert::DeviceCertificate::sign(
                            device_vk, device_name, now, &root,
                        );
                        drop(root);

                        let roster_doc = home
                            .document::<indras_sync_engine::device_roster::DeviceRoster>(
                                indras_sync_engine::device_roster::DEVICE_ROSTER_DOC_KEY,
                            )
                            .await
                            .map_err(|e| format!("Open device roster: {}", e))?;
                        let cert_clone = cert.clone();
                        roster_doc
                            .update(move |r| r.upsert(cert_clone))
                            .await
                            .map_err(|e| format!("Publish device roster: {}", e))?;

                        // Successfully signed and published the new
                        // device cert. No keystore re-auth needed —
                        // identity is now rooted in the roster, not
                        // the encrypted-at-rest PQ keys.
                        return Ok(subkey);
                    }
                    Err(_) => {
                        // Envelope is present but the reassembled key
                        // doesn't unseal it. Fall through to the
                        // legacy story-keystore path — might still
                        // work for hybrid accounts.
                    }
                }
            }
        }
    }

    // Legacy path (pre-B.4 accounts): the subkey is the keystore
    // encryption key. Re-auth against the stored token.
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

    let data_dir_owned = data_dir.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut keystore = StoryKeystore::new(&data_dir_owned);
        if !keystore.is_initialized() {
            return Err("No keystore to recover on this device".to_string());
        }
        keystore
            .authenticate(&subkey, token)
            .map_err(|e| format!("Recovered key did not unlock the keystore: {}", e))?;
        keystore
            .load_or_generate_pq_identity()
            .map_err(|e| format!("Couldn't load the recovered identity: {}", e))?;
        Ok(())
    })
    .await
    .map_err(|e| format!("task join error: {}", e))??;

    // Cache the subkey so this device can re-split without the story.
    let _ = save_subkey_cache(&data_dir, &subkey);

    Ok(subkey)
}

fn recovery_protocol_key_request(uid: &[u8; 32]) -> String {
    indras_sync_engine::recovery_protocol::recovery_request_doc_key(uid)
}

fn recovery_protocol_key_release(uid: &[u8; 32]) -> String {
    indras_sync_engine::recovery_protocol::share_release_doc_key(uid)
}

// ──────────────────────────────────────────────────────────────────
// Backup-Peer role management (Plan C user-facing)
// ──────────────────────────────────────────────────────────────────

/// Sender-side view of one peer's Backup-Peer assignment.
#[derive(Debug, Clone)]
pub struct OutgoingBackupPeer {
    pub peer_user_id_hex: String,
    pub peer_label: String,
    /// `true` when the sender has a currently-active assignment.
    pub active: bool,
    /// Wall-clock millis of the latest assignment.
    pub assigned_at_millis: i64,
}

/// Steward-side view of an incoming Backup-Peer role assignment.
#[derive(Debug, Clone)]
pub struct IncomingBackupRole {
    pub from_user_id_hex: String,
    pub from_display_name: String,
    pub responsibility_text: String,
    pub assigned_at_millis: i64,
    pub retired: bool,
}

/// Assign a DM peer as a Backup Peer — the account's file-shard
/// pipeline (Plan C C.4) will publish encrypted pieces into this
/// peer's DM realm.
pub async fn invite_backup_peer(
    network: Arc<IndrasNetwork>,
    peer_user_id_hex: &str,
) -> Result<(), String> {
    use indras_sync_engine::backup_peers::{
        backup_role_doc_key, BackupPeerAssignment, DEFAULT_BACKUP_RESPONSIBILITY,
    };

    let peer_uid = decode_user_id(peer_user_id_hex)
        .ok_or_else(|| "Invalid peer id".to_string())?;
    let realm_id = dm_realm_for_uid(&network, &peer_uid)
        .await
        .ok_or_else(|| "That friend isn't a direct-message peer yet.".to_string())?;
    let realm = network
        .get_realm_by_id(&realm_id)
        .ok_or_else(|| "Couldn't open the direct-message realm.".to_string())?;
    let my_uid = network.node().pq_identity().user_id();
    let my_name = network.display_name().unwrap_or_default();
    let key = backup_role_doc_key(&my_uid);
    let doc = realm
        .document::<BackupPeerAssignment>(&key)
        .await
        .map_err(|e| format!("Open backup-role doc: {e}"))?;

    let payload = BackupPeerAssignment {
        requester_user_id: my_uid,
        requester_display_name: my_name,
        responsibility_text: DEFAULT_BACKUP_RESPONSIBILITY.to_string(),
        shard_capacity_estimate: 0,
        assigned_at_millis: chrono::Utc::now().timestamp_millis(),
        retired: false,
    };
    doc.update(move |d| *d = payload)
        .await
        .map_err(|e| format!("Publish backup role: {e}"))?;
    Ok(())
}

/// Mark a Backup Peer assignment as retired; the peer's UI should
/// stop surfacing the role and the sender's save hook should skip
/// them on future shards.
pub async fn retire_backup_peer(
    network: Arc<IndrasNetwork>,
    peer_user_id_hex: &str,
) -> Result<(), String> {
    use indras_sync_engine::backup_peers::{backup_role_doc_key, BackupPeerAssignment};

    let peer_uid = decode_user_id(peer_user_id_hex)
        .ok_or_else(|| "Invalid peer id".to_string())?;
    let realm_id = dm_realm_for_uid(&network, &peer_uid)
        .await
        .ok_or_else(|| "That friend isn't a direct-message peer.".to_string())?;
    let realm = network
        .get_realm_by_id(&realm_id)
        .ok_or_else(|| "Couldn't open the direct-message realm.".to_string())?;
    let my_uid = network.node().pq_identity().user_id();
    let my_name = network.display_name().unwrap_or_default();
    let key = backup_role_doc_key(&my_uid);
    let doc = realm
        .document::<BackupPeerAssignment>(&key)
        .await
        .map_err(|e| format!("Open backup-role doc: {e}"))?;

    let payload = BackupPeerAssignment {
        requester_user_id: my_uid,
        requester_display_name: my_name,
        responsibility_text: String::new(),
        shard_capacity_estimate: 0,
        assigned_at_millis: chrono::Utc::now().timestamp_millis(),
        retired: true,
    };
    doc.update(move |d| *d = payload)
        .await
        .map_err(|e| format!("Retire backup role: {e}"))?;
    Ok(())
}

/// Enumerate every DM peer and the current state of their
/// Backup-Peer assignment (if any). Drives the Backup-plan
/// overlay's "People who hold copies of your files" list.
pub async fn list_outgoing_backup_peers(
    network: Arc<IndrasNetwork>,
) -> Vec<OutgoingBackupPeer> {
    use indras_sync_engine::backup_peers::{backup_role_doc_key, BackupPeerAssignment};

    let my_uid = network.node().pq_identity().user_id();
    let key = backup_role_doc_key(&my_uid);

    let mut out = Vec::new();
    for realm_id in network.conversation_realms() {
        if network.dm_peer_for_realm(&realm_id).is_none() {
            continue;
        }
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };

        // Resolve peer label via profile mirror.
        let peer_name = match network.dm_peer_for_realm(&realm_id) {
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

        let snapshot = match realm.document::<BackupPeerAssignment>(&key).await {
            Ok(d) => Some(d.read().await.clone()),
            Err(_) => None,
        };
        let (active, assigned_at_millis) = match snapshot {
            Some(s) if s.assigned_at_millis > 0 => (!s.retired, s.assigned_at_millis),
            _ => (false, 0),
        };

        for uid in peer_uids {
            let uid_hex = hex::encode(uid);
            let label = peer_name
                .clone()
                .unwrap_or_else(|| format!("Peer {}", &uid_hex[..8]));
            out.push(OutgoingBackupPeer {
                peer_user_id_hex: uid_hex,
                peer_label: label,
                active,
                assigned_at_millis,
            });
        }
    }
    out
}

/// Walk every DM realm and collect Backup-Peer assignments we've
/// received from our peers. Informational for the steward inbox.
pub async fn list_incoming_backup_roles(
    network: Arc<IndrasNetwork>,
) -> Vec<IncomingBackupRole> {
    use indras_sync_engine::backup_peers::{backup_role_doc_key, BackupPeerAssignment};

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
            let key = backup_role_doc_key(&peer_uid);
            let Ok(doc) = realm.document::<BackupPeerAssignment>(&key).await else {
                continue;
            };
            let snap = doc.read().await.clone();
            if snap.assigned_at_millis == 0 {
                continue;
            }
            out.push(IncomingBackupRole {
                from_user_id_hex: hex::encode(peer_uid),
                from_display_name: snap.requester_display_name,
                responsibility_text: snap.responsibility_text,
                assigned_at_millis: snap.assigned_at_millis,
                retired: snap.retired,
            });
        }
    }
    out
}

/// Publish one encrypted, erasure-coded shard of `file_bytes` into
/// each backup peer's DM realm. Uses the caller-supplied
/// `wrapping_key` (the Plan-B account wrapping key) to seal the
/// per-file symmetric key so the same key W that gates recovery
/// also gates file retrieval.
///
/// Returns the count of peers that actually received a shard —
/// unreachable peers are silently skipped. `data_threshold` must be
/// <= `peer_user_id_hexes.len()`.
pub async fn publish_file_shards(
    network: Arc<IndrasNetwork>,
    peer_user_id_hexes: &[String],
    wrapping_key: &[u8; 32],
    file_bytes: &[u8],
    file_label: String,
    data_threshold: u8,
) -> Result<usize, String> {
    use indras_sync_engine::file_shard::{
        file_shard_doc_key, prepare_file_shards, FileShard,
    };

    let total_peers = peer_user_id_hexes.len() as u8;
    if data_threshold == 0 {
        return Err("Shard threshold must be at least 1.".to_string());
    }
    if total_peers < data_threshold {
        return Err(format!(
            "Need at least {} backup peers; only {} provided.",
            data_threshold, total_peers
        ));
    }

    let file_id = *blake3::hash(file_bytes).as_bytes();
    let now = chrono::Utc::now().timestamp_millis();
    let prepared = prepare_file_shards(
        file_bytes,
        &file_id,
        file_label.clone(),
        wrapping_key,
        data_threshold,
        total_peers,
        now,
    )
    .map_err(|e| format!("Shard encode failed: {e}"))?;

    // Each peer gets one shard (matching the index).
    let mut delivered = 0usize;
    for (idx, (shard, peer_hex)) in prepared
        .shards
        .into_iter()
        .zip(peer_user_id_hexes.iter())
        .enumerate()
    {
        let Some(peer_uid) = decode_user_id(peer_hex) else {
            continue;
        };
        let Some(realm_id) = dm_realm_for_uid(&network, &peer_uid).await else {
            continue;
        };
        let Some(realm) = network.get_realm_by_id(&realm_id) else {
            continue;
        };
        let key = file_shard_doc_key(&file_id, idx as u8);
        let Ok(doc) = realm.document::<FileShard>(&key).await else {
            continue;
        };
        if doc.update(move |d| *d = shard).await.is_ok() {
            delivered += 1;
        }
    }

    // Update the home-realm backup index so future devices know
    // this file_id is out there and worth re-pulling.
    if delivered > 0 {
        use indras_sync_engine::file_backup_index::{
            FileBackupEntry, FileBackupIndex, FILE_BACKUP_INDEX_DOC_KEY,
        };
        if let Ok(home) = network.home_realm().await {
            if let Ok(doc) = home
                .document::<FileBackupIndex>(FILE_BACKUP_INDEX_DOC_KEY)
                .await
            {
                let entry = FileBackupEntry {
                    file_id,
                    label: file_label,
                    total_shards: total_peers,
                    data_threshold,
                    last_updated_at_millis: now,
                    tombstoned: false,
                };
                let _ = doc.update(move |idx| idx.upsert(entry)).await;
            }
        }
    }

    Ok(delivered)
}

/// Walk every file in the given vault directory and publish one
/// encrypted shard per file to each backup peer. Coarse-grained
/// MVP wiring — re-publishes every file on every call rather than
/// watching for diffs. Good enough for a user-triggered "Back up
/// my files" button; a finer-grained save-hook can replace it
/// later.
///
/// Skips hidden files (leading `.`) and non-regular files. Empty
/// vault is a no-op.
pub async fn publish_vault_backup(
    network: Arc<IndrasNetwork>,
    wrapping_key: &[u8; 32],
    vault_path: &std::path::Path,
    peer_user_id_hexes: Vec<String>,
    data_threshold: u8,
) -> Result<PublishVaultSummary, String> {
    let total_peers = peer_user_id_hexes.len() as u8;
    if total_peers < data_threshold {
        return Err(format!(
            "Need at least {} backup peers; only {} provided.",
            data_threshold, total_peers
        ));
    }
    if !vault_path.is_dir() {
        return Ok(PublishVaultSummary::default());
    }

    let mut summary = PublishVaultSummary::default();
    let entries = std::fs::read_dir(vault_path)
        .map_err(|e| format!("read vault dir: {e}"))?;
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || !path.is_file() {
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => {
                summary.failed.push(name.to_string());
                continue;
            }
        };
        match publish_file_shards(
            network.clone(),
            &peer_user_id_hexes,
            wrapping_key,
            &bytes,
            name.to_string(),
            data_threshold,
        )
        .await
        {
            Ok(delivered) => {
                summary.files_published += 1;
                summary.total_shards_delivered += delivered;
            }
            Err(_) => summary.failed.push(name.to_string()),
        }
    }
    Ok(summary)
}

/// Outcome of a full-vault backup pass.
#[derive(Debug, Clone, Default)]
pub struct PublishVaultSummary {
    pub files_published: usize,
    pub total_shards_delivered: usize,
    pub failed: Vec<String>,
}

/// Restore files from backup peers into `vault_path`.
///
/// Reads the account's home-realm [`FileBackupIndex`] to learn
/// which `file_id`s to expect, pulls shards from every DM realm,
/// and for each file with >= K intact shards reconstructs the
/// original plaintext and writes it under `{label}` inside the
/// vault. Missing files (below threshold) are left out of the
/// summary's `restored` list but do not fail the call.
///
/// Idempotent: running again after partial recovery picks up any
/// files that now have enough shards. Writes use
/// `{vault_path}/{label}`; if `label` is an absolute or parent-
/// escaping path we substitute a sanitized `{file_id_hex8}.bin`
/// name so a hostile sender can't land files outside the vault.
pub async fn repull_vault_backup(
    network: Arc<IndrasNetwork>,
    wrapping_key: &[u8; 32],
    vault_path: &std::path::Path,
) -> Result<RepullSummary, String> {
    use indras_sync_engine::file_backup_index::{FileBackupIndex, FILE_BACKUP_INDEX_DOC_KEY};
    use indras_sync_engine::file_shard::{
        file_shard_doc_key, reconstruct_file, FileShard,
    };

    let home = network
        .home_realm()
        .await
        .map_err(|e| format!("home realm unavailable: {e}"))?;
    let index = {
        let Ok(doc) = home
            .document::<FileBackupIndex>(FILE_BACKUP_INDEX_DOC_KEY)
            .await
        else {
            return Ok(RepullSummary::default());
        };
        doc.read().await.clone()
    };

    std::fs::create_dir_all(vault_path).map_err(|e| format!("create vault dir: {e}"))?;

    let mut summary = RepullSummary::default();
    for entry in index.active() {
        let total = entry.total_shards as usize;
        let mut collected: Vec<Option<FileShard>> = vec![None; total];
        // Scan every DM realm for shards of this file_id.
        for realm_id in network.conversation_realms() {
            if network.dm_peer_for_realm(&realm_id).is_none() {
                continue;
            }
            let Some(realm) = network.get_realm_by_id(&realm_id) else {
                continue;
            };
            for idx in 0..total {
                if collected[idx].is_some() {
                    continue;
                }
                let key = file_shard_doc_key(&entry.file_id, idx as u8);
                let Ok(doc) = realm.document::<FileShard>(&key).await else {
                    continue;
                };
                let snap = doc.read().await.clone();
                if snap.created_at_millis > 0 && !snap.shard_bytes.is_empty() {
                    collected[idx] = Some(snap);
                }
            }
        }

        let present = collected.iter().filter(|s| s.is_some()).count();
        if present < entry.data_threshold as usize {
            summary.missing.push(entry.label.clone());
            continue;
        }

        match reconstruct_file(collected, wrapping_key) {
            Ok(bytes) => {
                let safe_label = sanitize_label(&entry.label, &entry.file_id);
                let out_path = vault_path.join(&safe_label);
                if let Some(parent) = out_path.parent() {
                    if parent != vault_path {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }
                match std::fs::write(&out_path, &bytes) {
                    Ok(()) => {
                        summary.restored.push(safe_label);
                    }
                    Err(e) => {
                        summary.failed.push(format!("{}: {e}", entry.label));
                    }
                }
            }
            Err(e) => summary.failed.push(format!("{}: {e}", entry.label)),
        }
    }
    Ok(summary)
}

/// Summary of a `repull_vault_backup` pass.
#[derive(Debug, Clone, Default)]
pub struct RepullSummary {
    /// File labels (as landed on disk) that reconstructed cleanly.
    pub restored: Vec<String>,
    /// Files whose shard count is still below threshold.
    pub missing: Vec<String>,
    /// Files that had enough shards but failed to decrypt or write.
    pub failed: Vec<String>,
}

/// Coerce an index label into a safe filename within the vault
/// directory. Rejects parent-escaping and absolute paths.
fn sanitize_label(label: &str, file_id: &[u8; 32]) -> String {
    let trimmed = label.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || trimmed.contains("..")
    {
        let hex = hex::encode(file_id);
        return format!("{}.bin", &hex[..8]);
    }
    trimmed
        .chars()
        .map(|c| if "/\\\0".contains(c) { '_' } else { c })
        .collect()
}
