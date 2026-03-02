//! Periodic polling loop for contacts and DM realm invites.
//!
//! Extracts the 2-second polling loop from app.rs that handles:
//! - Contact discovery from the contacts realm
//! - DM realm invite checking (every ~10s)
//! - Periodic world view saves (every ~30s)

use indras_network::{IndrasNetwork, ArtifactStatus, HomeArtifactEntry, HomeRealm, Realm, EditableMessageType};
use indras_artifacts::Intention;
use crate::bridge::vault_bridge::VaultHandle;
use crate::state::workspace::PeerDisplayInfo;

/// Peer color classes used for display.
pub const PEER_COLORS: &[&str] = &[
    "peer-dot-sage", "peer-dot-zeph", "peer-dot-rose",
    "peer-dot-sage", "peer-dot-zeph", "peer-dot-rose",
];

/// A new contact discovered during polling.
#[derive(Clone, Debug)]
pub struct NewContact {
    /// Display info for the new contact.
    pub info: PeerDisplayInfo,
}

/// A realm invite received via DM.
#[derive(Clone, Debug)]
pub struct ReceivedInvite {
    /// The invite code to join the realm.
    pub invite_code: String,
    /// The name of the shared intention/realm.
    pub name: String,
    /// Description of the intention.
    pub description: String,
    /// The peer who sent the invite.
    pub peer_id: [u8; 32],
}

/// Poll the contacts realm and return the current contact list.
///
/// Returns `None` if the contacts realm is unavailable or unchanged.
/// Returns `Some(entries)` with the full contact list when the count changes.
pub async fn poll_contacts(
    network: &IndrasNetwork,
    current_count: usize,
) -> Option<Vec<PeerDisplayInfo>> {
    let contacts_realm = network.contacts_realm().await?;
    let doc = contacts_realm.contacts().await.ok()?;
    let data = doc.read().await;

    if data.contacts.len() == current_count {
        return None;
    }

    let entries: Vec<PeerDisplayInfo> = data.contacts.iter().enumerate().map(|(i, (mid, entry))| {
        let name = entry.display_name.clone().unwrap_or_else(|| {
            mid.iter().take(4).map(|b| format!("{:02x}", b)).collect()
        });
        let letter = name.chars().next().unwrap_or('?').to_string();
        let color = PEER_COLORS[i % PEER_COLORS.len()].to_string();
        PeerDisplayInfo {
            name,
            letter,
            color_class: color,
            online: true,
            player_id: *mid,
        }
    }).collect();

    Some(entries)
}

/// Check DM chats for incoming realm invites from known peers.
///
/// Scans each peer's DM realm for `RealmInvite` messages that haven't
/// been processed yet. Returns a list of invites to join.
pub async fn check_dm_invites(
    network: &IndrasNetwork,
    peers: &[PeerDisplayInfo],
    my_name: &str,
    processed_invites: &mut std::collections::HashSet<String>,
    dm_realms: &mut std::collections::HashMap<[u8; 32], Realm>,
) -> Vec<ReceivedInvite> {
    let mut invites = Vec::new();

    for peer_entry in peers {
        // Reuse persistent DM realm so Document listener stays alive across polls
        if !dm_realms.contains_key(&peer_entry.player_id) {
            if let Ok(r) = network.connect(peer_entry.player_id).await {
                dm_realms.insert(peer_entry.player_id, r);
            }
        }
        if let Some(dm_realm) = dm_realms.get(&peer_entry.player_id) {
            if let Ok(chat_doc) = dm_realm.chat_doc().await {
                let _ = chat_doc.refresh().await;
                let data = chat_doc.read().await;
                for msg in data.visible_messages() {
                    if msg.author == my_name { continue; }
                    if let EditableMessageType::RealmInvite {
                        ref invite_code, ref name, ref description, ..
                    } = msg.message_type {
                        if processed_invites.contains(&msg.id) { continue; }
                        processed_invites.insert(msg.id.clone());

                        invites.push(ReceivedInvite {
                            invite_code: invite_code.clone(),
                            name: name.clone(),
                            description: description.clone(),
                            peer_id: peer_entry.player_id,
                        });
                    }
                }
            }
        }
    }

    invites
}

/// Join a received realm invite, creating the local intention artifact.
///
/// Returns the realm and intention artifact ID on success.
pub async fn join_invite(
    network: &IndrasNetwork,
    vault_handle: &VaultHandle,
    invite: &ReceivedInvite,
) -> Option<(Realm, indras_artifacts::ArtifactId)> {
    let shared_realm = network.join(&invite.invite_code).await.ok()?;

    let mut vault = vault_handle.vault.lock().await;
    let join_now = chrono::Utc::now().timestamp_millis();
    let audience = vec![vault_handle.player_id, invite.peer_id];

    let intention = Intention::create(&mut vault, &invite.description, audience, join_now).ok()?;
    let root_id = vault.root.id.clone();
    let pos = vault.get_artifact(&root_id)
        .ok().flatten()
        .map(|a| a.references.len() as u64)
        .unwrap_or(0);
    vault.compose(&root_id, intention.id, pos, Some(invite.name.clone())).ok()?;

    Some((shared_realm, intention.id))
}

/// Store an artifact entry in the home realm's artifact index.
pub async fn store_in_artifact_index(
    home: &HomeRealm,
    id: indras_artifacts::ArtifactId,
    name: &str,
) {
    if let Ok(doc) = home.artifact_index().await {
        let now = chrono::Utc::now().timestamp_millis();
        let entry = HomeArtifactEntry {
            id,
            name: name.to_string(),
            mime_type: None,
            size: 0,
            created_at: now,
            encrypted_key: None,
            status: ArtifactStatus::Active,
            grants: vec![],
            provenance: None,
            location: None,
        };
        let _ = doc.update(|index| { index.store(entry); }).await;
    }
}
