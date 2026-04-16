//! Mirrors the local user's grant-visible profile fields into every DM realm
//! so the counterparty can read what's been shared with them without an HTTP
//! fetch against the homepage server.
//!
//! Each side writes only its own slot, keyed by hex member id, so the two
//! per-peer slots in a DM realm never collide. Grant filtering happens at
//! write time: ungranted string fields are written as empty, ungranted
//! `bio` is written as `None`. The reader treats those as "not shared."
//!
//! Liveness is handled separately by [`crate::heartbeat`] over realm
//! message events — the mirror only writes when content actually changes
//! so the profile doc's history doesn't accumulate liveness churn.

use std::sync::Arc;
use std::time::Duration;

use indras_artifacts::{AccessGrant, ArtifactId};
use indras_homepage::{fields as field_names, grants, profile_field_artifact_id};
use indras_network::IndrasNetwork;
use indras_network::artifact_index::ArtifactIndex;
use indras_sync_engine::ProfileIdentityDocument;

/// Document key under which a peer publishes their own profile mirror in a
/// shared realm. Includes the writer's hex member id so the two slots in a
/// 2-peer DM realm never overwrite each other.
pub fn peer_profile_doc_key(member_id: &[u8; 32]) -> String {
    let hex: String = member_id.iter().map(|b| format!("{b:02x}")).collect();
    format!("_peer_profile:{hex}")
}

/// Spawn a background loop that re-publishes the local profile mirror into
/// every DM realm at a fixed cadence. Cheap when content hasn't changed
/// (skip-if-unchanged in `publish_mirror`).
pub fn start_profile_mirror_loop(network: Arc<IndrasNetwork>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(5));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            publish_mirror(&network).await;
        }
    });
}

/// Publish the local profile mirror into every DM realm I'm currently in.
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

        let filtered = build_filtered(&identity, &grants_by_field, &me, &peer_id, now);

        let Some(realm) = network.get_realm_by_id(&realm_id) else { continue };
        let Ok(doc) = realm.document::<ProfileIdentityDocument>(&key).await else { continue };

        // Skip writes when nothing changed so the doc's history doesn't
        // accumulate. Liveness is signaled via `crate::heartbeat`, not by
        // bumping `updated_at` here.
        let new = filtered;
        let res = doc
            .update(move |d| {
                let changed = d.display_name != new.display_name
                    || d.username != new.username
                    || d.bio != new.bio
                    || d.public_key != new.public_key;
                if changed {
                    d.display_name = new.display_name;
                    d.username = new.username;
                    d.bio = new.bio;
                    d.public_key = new.public_key;
                    d.updated_at = now;
                }
            })
            .await;
        if let Err(e) = res {
            tracing::warn!("profile mirror update failed: {e}");
        }
    }
}

/// Build the per-peer filtered identity. Fields the peer can't see are
/// written as empty strings (or `None` for `bio`).
fn build_filtered(
    identity: &ProfileIdentityDocument,
    grants_by_field: &[(&'static str, Vec<AccessGrant>)],
    me: &[u8; 32],
    peer: &[u8; 32],
    now: i64,
) -> ProfileIdentityDocument {
    let mut out = ProfileIdentityDocument {
        updated_at: now,
        ..Default::default()
    };
    for (name, g) in grants_by_field {
        if !grants::can_view(Some(peer), me, g, now) {
            continue;
        }
        if *name == field_names::DISPLAY_NAME {
            out.display_name = identity.display_name.clone();
        } else if *name == field_names::USERNAME {
            out.username = identity.username.clone();
        } else if *name == field_names::BIO {
            out.bio = identity.bio.clone();
        } else if *name == field_names::PUBLIC_KEY {
            out.public_key = identity.public_key.clone();
        }
    }
    out
}

fn field_grants_snapshot(
    me: &[u8; 32],
    guard: &ArtifactIndex,
) -> Vec<(&'static str, Vec<AccessGrant>)> {
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

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
