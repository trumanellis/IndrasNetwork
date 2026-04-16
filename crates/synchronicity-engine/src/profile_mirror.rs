//! Mirrors the local user's grant-visible profile fields into every DM realm
//! so each connected peer can read what's been shared with them without an
//! HTTP fetch against the homepage server.
//!
//! Each side writes only its own slot, keyed by hex member id, so the two
//! per-peer slots in a DM realm never collide. Grant filtering happens at
//! write time, so the reader just renders whatever lands in its slot.

use std::sync::Arc;
use std::time::Duration;

use indras_artifacts::{AccessGrant, ArtifactId};
use indras_homepage::{fields as field_names, grants, profile_field_artifact_id};
use indras_network::IndrasNetwork;
use indras_network::artifact_index::ArtifactIndex;
use indras_sync_engine::{HomepageField, HomepageProfileDocument, ProfileIdentityDocument};

/// Document key under which a peer publishes their own profile mirror in a
/// shared realm. Includes the writer's hex member id so the two slots in a
/// 2-peer DM realm never overwrite each other.
pub fn peer_profile_doc_key(member_id: &[u8; 32]) -> String {
    let hex: String = member_id.iter().map(|b| format!("{b:02x}")).collect();
    format!("_peer_profile:{hex}")
}

/// Spawn a background loop that re-publishes the local profile mirror into
/// every DM realm at a fixed cadence. Polling matches the homepage refresh
/// loop so newly granted fields appear within a few seconds on the peer side.
pub fn start_profile_mirror_loop(network: Arc<IndrasNetwork>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(3));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            publish_mirror(&network).await;
        }
    });
}

/// Publish the local profile mirror into every DM realm I'm currently in.
///
/// For each DM realm, only the fields the counterparty is currently granted
/// to view are written. Unchanged docs are skipped to avoid bumping LWW.
async fn publish_mirror(network: &Arc<IndrasNetwork>) {
    let me = network.id();

    let Ok(home) = network.home_realm().await else { return };
    let Ok(identity_doc) = home.document::<ProfileIdentityDocument>("_profile_identity").await
    else {
        return;
    };
    let identity = identity_doc.read().await.clone();

    let grants_by_field = match home.artifact_index().await {
        Ok(doc) => {
            let guard = doc.read().await;
            field_grants_snapshot(&me, &guard)
        }
        Err(_) => return,
    };

    let now = now_secs();
    let key = peer_profile_doc_key(&me);

    for realm_id in network.conversation_realms() {
        let Some(peer_id) = network.dm_peer_for_realm(&realm_id) else { continue };

        let visible_fields: Vec<HomepageField> = grants_by_field
            .iter()
            .filter(|(_, g)| grants::can_view(Some(&peer_id), &me, g, now))
            .map(|(name, _)| HomepageField {
                name: (*name).to_string(),
                value: identity_value(&identity, name),
                grants_json: "[]".to_string(),
            })
            .collect();

        let Some(realm) = network.get_realm_by_id(&realm_id) else { continue };
        let Ok(doc) = realm.document::<HomepageProfileDocument>(&key).await else { continue };

        let new_fields = visible_fields;
        let res = doc
            .update(move |d| {
                if d.fields != new_fields {
                    d.fields = new_fields;
                    d.updated_at = now;
                }
            })
            .await;
        if let Err(e) = res {
            tracing::warn!("profile mirror update for realm failed: {e}");
        }
    }
}

fn field_grants_snapshot(me: &[u8; 32], guard: &ArtifactIndex) -> Vec<(&'static str, Vec<AccessGrant>)> {
    [
        field_names::DISPLAY_NAME,
        field_names::USERNAME,
        field_names::BIO,
        field_names::PUBLIC_KEY,
    ]
    .iter()
    .map(|name| {
        let aid = ArtifactId::Doc(profile_field_artifact_id(me, name));
        let grants = guard
            .get(&aid)
            .map(|entry| entry.grants.clone())
            .unwrap_or_default();
        (*name, grants)
    })
    .collect()
}

fn identity_value(identity: &ProfileIdentityDocument, name: &str) -> String {
    if name == field_names::DISPLAY_NAME {
        identity.display_name.clone()
    } else if name == field_names::USERNAME {
        identity.username.clone()
    } else if name == field_names::BIO {
        identity.bio.clone().unwrap_or_default()
    } else if name == field_names::PUBLIC_KEY {
        identity.public_key.clone()
    } else {
        String::new()
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
