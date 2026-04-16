//! Starts an Axum-based `HomepageServer` for the local identity.
//!
//! The server is the outward-facing surface for a peer's profile: it serves
//! grant-filtered HTML at `/` and JSON at `/api/profile`, so other peers can
//! read whatever the owner has made visible to them. Each SE instance binds
//! to `INDRAS_HOMEPAGE_PORT` (default 3000) on 127.0.0.1.
//!
//! The field values themselves are populated by the gift-cycle-style polling
//! loop — not yet ported. Until that lands, the server serves whatever is
//! already persisted in `HomepageProfileDocument` plus any visibility
//! changes the user makes through the profile modal.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use indras_artifacts::AccessGrant;
use indras_homepage::{
    fields as field_names, profile_field_artifact_id, ContentArtifact, HomepageServer,
    HomepageStore, ProfileFieldArtifact,
};
use indras_network::IndrasNetwork;
use indras_sync_engine::{HomepageField, HomepageProfileDocument, ProfileIdentityDocument};
use tokio::sync::RwLock;

/// Handles onto the running server's in-memory state. Hold these in app
/// state so the polling loop (future) can write computed field values.
pub struct HomepageHandles {
    /// Writable handle to the profile fields served at `/api/profile`.
    pub fields: Arc<RwLock<Vec<ProfileFieldArtifact>>>,
    /// Writable handle to content artifacts (images, attachments).
    pub artifacts: Arc<RwLock<Vec<ContentArtifact>>>,
}

/// Resolve the port this instance should bind.
fn homepage_port() -> u16 {
    std::env::var("INDRAS_HOMEPAGE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000)
}

/// Spin up the homepage HTTP server for the local identity.
///
/// Returns handles to the in-memory state so callers can feed computed field
/// values into the live-served endpoint. Failures during store creation or
/// server start are logged and treated as non-fatal — the app keeps running,
/// the profile just isn't exposed.
pub async fn start_homepage_server(
    network: &Arc<IndrasNetwork>,
    data_dir: &Path,
) -> Option<HomepageHandles> {
    let member_id = network.id();

    // Persistent backing store so field values survive restarts.
    let store: Option<Arc<dyn HomepageStore>> = match indras_relay::blob_store::BlobStore::open(
        &data_dir.join("homepage-events.redb"),
    ) {
        Ok(bs) => Some(Arc::new(
            indras_relay::blob_homepage::BlobStoreHomepageStore::new(Arc::new(bs), &member_id),
        ) as Arc<dyn HomepageStore>),
        Err(e) => {
            tracing::warn!("homepage blob store unavailable: {e}");
            None
        }
    };

    let mut server = HomepageServer::new(member_id);
    if let Some(ref s) = store {
        server = server.with_store(Arc::clone(s));
    }
    let fields = server.fields_handle();
    let artifacts = server.artifacts_handle();

    // Pre-populate from the persisted CRDT so the server has something to
    // serve before the polling loop (if/when ported) catches up.
    if let Ok(home) = network.home_realm().await {
        if let Ok(doc) = home
            .document::<indras_sync_engine::HomepageProfileDocument>("_homepage_profile")
            .await
        {
            let snapshot = doc.read().await;
            if !snapshot.fields.is_empty() {
                let seeded: Vec<ProfileFieldArtifact> = snapshot
                    .fields
                    .iter()
                    .map(|f| ProfileFieldArtifact {
                        field_name: f.name.clone(),
                        display_value: f.value.clone(),
                        grants: serde_json::from_str(&f.grants_json).unwrap_or_default(),
                    })
                    .collect();
                *fields.write().await = seeded;
            }
        }
    }

    let port = homepage_port();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    tokio::spawn(async move {
        if let Err(e) = server.serve(addr).await {
            tracing::error!("homepage server failed on port {port}: {e}");
        }
    });
    tracing::info!("homepage server listening at http://127.0.0.1:{port}");

    // Refresh loop: every 2s re-read the profile identity + grants and write
    // them into both the HomepageProfileDocument CRDT and the live fields
    // handle the server reads from. This is a minimal port of the gift-cycle
    // polling loop that covers only the user-editable fields; computed stats
    // (intention count, tokens, etc.) will land in a follow-up.
    let refresh_network = Arc::clone(network);
    let refresh_fields = Arc::clone(&fields);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(2));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            refresh_identity_fields(&refresh_network, &refresh_fields).await;
        }
    });

    Some(HomepageHandles { fields, artifacts })
}

/// Mirror the local user's identity + grants into `HomepageProfileDocument`
/// and the in-memory fields handle the server serves from.
async fn refresh_identity_fields(
    network: &Arc<IndrasNetwork>,
    fields_handle: &Arc<RwLock<Vec<ProfileFieldArtifact>>>,
) {
    let Ok(home) = network.home_realm().await else { return };

    let identity = match home.document::<ProfileIdentityDocument>("_profile_identity").await {
        Ok(d) => d.read().await.clone(),
        Err(_) => return,
    };
    let member_id = network.id();

    // Pull grants per field from the artifact index (default to empty).
    let grants_by_field = match home.artifact_index().await {
        Ok(doc) => {
            let guard = doc.read().await;
            [
                field_names::DISPLAY_NAME,
                field_names::USERNAME,
                field_names::BIO,
                field_names::PUBLIC_KEY,
            ]
            .iter()
            .map(|name| {
                let aid = indras_artifacts::ArtifactId::Doc(profile_field_artifact_id(&member_id, name));
                let grants = guard
                    .get(&aid)
                    .map(|entry| entry.grants.clone())
                    .unwrap_or_default();
                (*name, grants)
            })
            .collect::<Vec<_>>()
        }
        Err(_) => [
            field_names::DISPLAY_NAME,
            field_names::USERNAME,
            field_names::BIO,
            field_names::PUBLIC_KEY,
        ]
        .iter()
        .map(|n| (*n, Vec::<AccessGrant>::new()))
        .collect(),
    };

    let entries: Vec<ProfileFieldArtifact> = grants_by_field
        .into_iter()
        .map(|(name, grants)| {
            let value = match name {
                field_names::DISPLAY_NAME => identity.display_name.clone(),
                field_names::USERNAME => identity.username.clone(),
                field_names::BIO => identity.bio.clone().unwrap_or_default(),
                field_names::PUBLIC_KEY => identity.public_key.clone(),
                _ => String::new(),
            };
            ProfileFieldArtifact {
                field_name: name.to_string(),
                display_value: value,
                grants,
            }
        })
        .collect();

    // Update the live handle so the next HTTP request sees fresh values.
    {
        let mut write = fields_handle.write().await;
        *write = entries.clone();
    }

    // Persist into the CRDT doc so the snapshot survives restarts and syncs
    // to other devices. Only write when something actually changed.
    if let Ok(doc) = home.document::<HomepageProfileDocument>("_homepage_profile").await {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let _ = doc
            .update(move |d| {
                let new_fields: Vec<HomepageField> = entries
                    .iter()
                    .map(|e| HomepageField {
                        name: e.field_name.clone(),
                        value: e.display_value.clone(),
                        grants_json: serde_json::to_string(&e.grants).unwrap_or_else(|_| "[]".into()),
                    })
                    .collect();
                if d.fields != new_fields {
                    d.fields = new_fields;
                    d.updated_at = now;
                }
            })
            .await;
    }
}
