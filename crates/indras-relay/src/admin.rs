//! HTTP admin API for monitoring and management
//!
//! Provides health checks, statistics, and peer management
//! endpoints via axum with bearer token authentication.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use indras_core::identity::PeerIdentity;

use crate::auth::AuthService;
use crate::blob_store::BlobStore;
use crate::config::{QuotaConfig, RelayConfig, StorageConfig, TierConfig};
use crate::quota::{QuotaManager, TieredQuotaManager};
use crate::registration::RegistrationState;

/// Shared state for admin API handlers
pub struct AdminState {
    /// Live configuration — readers take the read lock; PUT /config takes the write lock.
    pub config: Arc<RwLock<RelayConfig>>,
    /// On-disk path to persist config changes to. `None` disables persistence
    /// (in-memory edits still apply).
    pub config_path: Option<PathBuf>,
    pub blob_store: Arc<BlobStore>,
    pub registrations: Arc<RegistrationState>,
    pub auth: Arc<AuthService>,
    pub quota: Arc<QuotaManager>,
    pub tiered_quota: Arc<TieredQuotaManager>,
    pub started_at: Instant,
}

/// Health check response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_secs: u64,
    pub display_name: String,
}

/// Statistics response
#[derive(Serialize)]
pub struct StatsResponse {
    pub peer_count: usize,
    pub interface_count: usize,
    pub total_events: usize,
    pub total_storage_bytes: u64,
    /// Per-tier storage usage
    pub self_bytes: u64,
    pub connections_bytes: u64,
    pub public_bytes: u64,
}

/// Peer info for admin API
#[derive(Serialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub display_name: Option<String>,
    pub interface_count: usize,
    pub registered_at: String,
    pub last_seen: String,
    /// Storage tiers this peer has access to
    pub granted_tiers: Vec<String>,
}

/// Interface info for admin API
#[derive(Serialize)]
pub struct InterfaceInfo {
    pub interface_id: String,
    pub event_count: usize,
    pub storage_bytes: u64,
}

/// Contacts list response
#[derive(Serialize)]
pub struct ContactsResponse {
    pub contacts: Vec<String>,
    pub count: usize,
}

/// Request body for replacing the contacts list
#[derive(Deserialize)]
pub struct UpdateContactsRequest {
    pub contacts: Vec<String>,
}

/// Redacted projection of `RelayConfig` returned to admin clients.
///
/// `admin_token` is never included. `data_dir`, `admin_bind`, and
/// `owner_player_id` are read-only and out-of-scope for runtime edits, but
/// are surfaced so operators can see the full picture.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelayConfigView {
    pub data_dir: PathBuf,
    pub display_name: String,
    pub admin_bind: std::net::SocketAddr,
    pub owner_player_id: Option<String>,
    pub quota: QuotaConfig,
    pub storage: StorageConfig,
    pub tiers: TierConfig,
}

impl RelayConfigView {
    pub fn from_config(c: &RelayConfig) -> Self {
        Self {
            data_dir: c.data_dir.clone(),
            display_name: c.display_name.clone(),
            admin_bind: c.admin_bind,
            owner_player_id: c.owner_player_id.clone(),
            quota: c.quota.clone(),
            storage: c.storage.clone(),
            tiers: c.tiers.clone(),
        }
    }
}

/// Sub-patch for `QuotaConfig`. All fields optional; only set fields are applied.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct QuotaConfigPatch {
    pub default_max_bytes_per_peer: Option<u64>,
    pub default_max_interfaces_per_peer: Option<usize>,
    pub global_max_bytes: Option<u64>,
}

/// Sub-patch for `StorageConfig`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct StorageConfigPatch {
    pub default_event_ttl_days: Option<u64>,
    pub max_event_ttl_days: Option<u64>,
    pub cleanup_interval_secs: Option<u64>,
}

/// Sub-patch for `TierConfig`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct TierConfigPatch {
    pub self_max_bytes: Option<u64>,
    pub self_ttl_days: Option<u64>,
    pub self_max_interfaces: Option<usize>,
    pub connections_max_bytes: Option<u64>,
    pub connections_ttl_days: Option<u64>,
    pub connections_max_interfaces: Option<usize>,
    pub public_max_bytes: Option<u64>,
    pub public_ttl_days: Option<u64>,
    pub public_max_interfaces: Option<usize>,
}

/// PUT /config request body. Out-of-scope fields (`data_dir`, `admin_bind`,
/// `admin_token`, `owner_player_id`) are absent by construction — sending
/// them yields an unknown-field error.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct RelayConfigPatch {
    pub display_name: Option<String>,
    pub quota: Option<QuotaConfigPatch>,
    pub storage: Option<StorageConfigPatch>,
    pub tiers: Option<TierConfigPatch>,
}

impl RelayConfigPatch {
    /// Apply this patch to `config` in place.
    pub fn apply_to(&self, config: &mut RelayConfig) {
        if let Some(name) = &self.display_name {
            config.display_name = name.clone();
        }
        if let Some(q) = &self.quota {
            if let Some(v) = q.default_max_bytes_per_peer {
                config.quota.default_max_bytes_per_peer = v;
            }
            if let Some(v) = q.default_max_interfaces_per_peer {
                config.quota.default_max_interfaces_per_peer = v;
            }
            if let Some(v) = q.global_max_bytes {
                config.quota.global_max_bytes = v;
            }
        }
        if let Some(s) = &self.storage {
            if let Some(v) = s.default_event_ttl_days {
                config.storage.default_event_ttl_days = v;
            }
            if let Some(v) = s.max_event_ttl_days {
                config.storage.max_event_ttl_days = v;
            }
            if let Some(v) = s.cleanup_interval_secs {
                config.storage.cleanup_interval_secs = v;
            }
        }
        if let Some(t) = &self.tiers {
            if let Some(v) = t.self_max_bytes {
                config.tiers.self_max_bytes = v;
            }
            if let Some(v) = t.self_ttl_days {
                config.tiers.self_ttl_days = v;
            }
            if let Some(v) = t.self_max_interfaces {
                config.tiers.self_max_interfaces = v;
            }
            if let Some(v) = t.connections_max_bytes {
                config.tiers.connections_max_bytes = v;
            }
            if let Some(v) = t.connections_ttl_days {
                config.tiers.connections_ttl_days = v;
            }
            if let Some(v) = t.connections_max_interfaces {
                config.tiers.connections_max_interfaces = v;
            }
            if let Some(v) = t.public_max_bytes {
                config.tiers.public_max_bytes = v;
            }
            if let Some(v) = t.public_ttl_days {
                config.tiers.public_ttl_days = v;
            }
            if let Some(v) = t.public_max_interfaces {
                config.tiers.public_max_interfaces = v;
            }
        }
    }
}

/// Build the admin API router
pub fn admin_router(state: Arc<AdminState>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/stats", get(stats_handler))
        .route("/peers", get(peers_handler))
        .route("/interfaces", get(interfaces_handler))
        .route("/contacts", get(contacts_handler).put(update_contacts_handler))
        .route("/config", get(get_config_handler).put(put_config_handler))
        .with_state(state)
}

/// Constant-time string comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Validate bearer token from request headers
fn validate_token(headers: &HeaderMap, expected: &str) -> Result<(), StatusCode> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !auth.starts_with("Bearer ") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let token = &auth[7..];
    if !constant_time_eq(token.as_bytes(), expected.as_bytes()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

/// Read the current admin token from shared config.
async fn current_admin_token(state: &AdminState) -> String {
    state.config.read().await.admin_token.clone()
}

/// GET /health — status and uptime (no auth required)
async fn health_handler(State(state): State<Arc<AdminState>>) -> Json<HealthResponse> {
    let display_name = state.config.read().await.display_name.clone();
    Json(HealthResponse {
        status: "ok".to_string(),
        uptime_secs: state.started_at.elapsed().as_secs(),
        display_name,
    })
}

/// GET /stats — peer count, interface count, storage usage
async fn stats_handler(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<StatsResponse>, StatusCode> {
    let token = current_admin_token(&state).await;
    validate_token(&headers, &token)?;

    let total_events = state.blob_store.event_count().unwrap_or(0);
    let total_storage = state.blob_store.total_usage_bytes().unwrap_or(0);

    use indras_transport::protocol::StorageTier;
    let self_bytes = state.blob_store.tier_usage_bytes(StorageTier::Self_).unwrap_or(0);
    let connections_bytes = state.blob_store.tier_usage_bytes(StorageTier::Connections).unwrap_or(0);
    let public_bytes = state.blob_store.tier_usage_bytes(StorageTier::Public).unwrap_or(0);

    Ok(Json(StatsResponse {
        peer_count: state.registrations.peer_count(),
        interface_count: state.registrations.interface_count(),
        total_events,
        total_storage_bytes: total_storage,
        self_bytes,
        connections_bytes,
        public_bytes,
    }))
}

/// GET /peers — registered peer list
async fn peers_handler(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<PeerInfo>>, StatusCode> {
    let token = current_admin_token(&state).await;
    validate_token(&headers, &token)?;

    let peers = state
        .registrations
        .registered_peers()
        .into_iter()
        .map(|p| {
            let granted_tiers = state
                .auth
                .get_session(&p.peer_id)
                .map(|s| s.granted_tiers.iter().map(|t| format!("{t:?}")).collect())
                .unwrap_or_default();
            PeerInfo {
                peer_id: hex::encode(&p.peer_id.as_bytes()),
                display_name: p.display_name,
                interface_count: p.interfaces.len(),
                registered_at: p.registered_at.to_rfc3339(),
                last_seen: p.last_seen.to_rfc3339(),
                granted_tiers,
            }
        })
        .collect();

    Ok(Json(peers))
}

/// GET /interfaces — cached interface list with event counts
async fn interfaces_handler(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<InterfaceInfo>>, StatusCode> {
    let token = current_admin_token(&state).await;
    validate_token(&headers, &token)?;

    let interfaces = state
        .registrations
        .registered_interfaces()
        .into_iter()
        .map(|iface| {
            let event_count = state
                .blob_store
                .interface_event_count(&iface)
                .unwrap_or(0);
            let storage_bytes = state
                .blob_store
                .interface_usage_bytes(&iface)
                .unwrap_or(0);
            InterfaceInfo {
                interface_id: hex::encode(&iface.0),
                event_count,
                storage_bytes,
            }
        })
        .collect();

    Ok(Json(interfaces))
}

/// GET /contacts — current contact list as hex strings
async fn contacts_handler(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<ContactsResponse>, StatusCode> {
    let token = current_admin_token(&state).await;
    validate_token(&headers, &token)?;

    let ids = state.auth.contact_ids();
    let contacts: Vec<String> = ids.iter().map(|id| hex::encode(id)).collect();
    let count = contacts.len();

    Ok(Json(ContactsResponse { contacts, count }))
}

/// PUT /contacts — replace the contact list
async fn update_contacts_handler(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
    Json(body): Json<UpdateContactsRequest>,
) -> Result<Json<ContactsResponse>, StatusCode> {
    let token = current_admin_token(&state).await;
    validate_token(&headers, &token)?;

    let parsed: Vec<[u8; 32]> = body
        .contacts
        .iter()
        .filter_map(|h| parse_hex_32(h))
        .collect();

    let count = parsed.len();
    state.auth.sync_contacts(parsed);

    let data_dir = state.config.read().await.data_dir.clone();
    let contacts_path = data_dir.join("contacts.json");
    state
        .auth
        .save_contacts(&contacts_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let contacts: Vec<String> = state.auth.contact_ids().iter().map(|id| hex::encode(id)).collect();

    Ok(Json(ContactsResponse { contacts, count }))
}

/// GET /config — current live configuration with `admin_token` redacted.
async fn get_config_handler(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<RelayConfigView>, StatusCode> {
    let token = current_admin_token(&state).await;
    validate_token(&headers, &token)?;

    let guard = state.config.read().await;
    Ok(Json(RelayConfigView::from_config(&guard)))
}

/// PUT /config — merge a patch into the live configuration.
///
/// Validates the merged config, persists to disk when `config_path` is set,
/// then updates dependent runtime managers so changes take effect immediately.
/// Returns the redacted updated configuration.
async fn put_config_handler(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
    Json(patch): Json<RelayConfigPatch>,
) -> Result<Json<RelayConfigView>, StatusCode> {
    let token = current_admin_token(&state).await;
    validate_token(&headers, &token)?;

    let mut guard = state.config.write().await;

    // Build candidate by cloning, applying, validating — so a bad patch
    // leaves the live config untouched.
    let mut candidate = guard.clone();
    patch.apply_to(&mut candidate);

    if let Err(e) = candidate.validate() {
        tracing::warn!(error = %e, "PUT /config rejected: validation failed");
        return Err(StatusCode::BAD_REQUEST);
    }

    if let Some(path) = &state.config_path {
        if let Err(e) = candidate.save_to_file(path) {
            tracing::error!(error = %e, "PUT /config: failed to persist");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    // Commit to live config.
    *guard = candidate;

    // Propagate to dependent runtime managers.
    state.quota.update_config(guard.quota.clone());
    state.tiered_quota.update_config(guard.tiers.clone());

    let view = RelayConfigView::from_config(&guard);
    Ok(Json(view))
}

/// Parse a 64-character hex string into a 32-byte array
fn parse_hex_32(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

/// Hex encoding helper
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthService;
    use crate::blob_store::BlobStore;
    use crate::config::RelayConfig;
    use crate::quota::{QuotaManager, TieredQuotaManager};
    use crate::registration::RegistrationState;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state(
        config: RelayConfig,
        config_path: Option<PathBuf>,
    ) -> Arc<AdminState> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(&config.data_dir).ok();
        let db_path = dir.path().join("events.redb");
        let blob_store = Arc::new(BlobStore::open(&db_path).unwrap());
        let registrations = Arc::new(RegistrationState::new(dir.path().join("reg.json")));
        let auth = Arc::new(AuthService::new(&config));
        let quota = Arc::new(QuotaManager::new(config.quota.clone()));
        let tiered_quota = Arc::new(TieredQuotaManager::new(config.tiers.clone()));

        let state = Arc::new(AdminState {
            config: Arc::new(RwLock::new(config)),
            config_path,
            blob_store,
            registrations,
            auth,
            quota,
            tiered_quota,
            started_at: Instant::now(),
        });
        // Keep tempdir alive for the test by leaking — only used in unit tests.
        std::mem::forget(dir);
        state
    }

    fn bearer(token: &str) -> String {
        format!("Bearer {token}")
    }

    #[tokio::test]
    async fn get_config_redacts_admin_token() {
        let mut cfg = RelayConfig::default();
        cfg.admin_token = "super-secret-token-value".to_string();
        let state = test_state(cfg, None);
        let app = admin_router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/config")
                    .header("authorization", bearer("super-secret-token-value"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(
            !body.contains("super-secret-token-value"),
            "admin_token leaked in GET /config: {body}"
        );
        assert!(
            !body.contains("admin_token"),
            "admin_token field leaked in GET /config: {body}"
        );
    }

    #[tokio::test]
    async fn put_config_updates_display_name() {
        let state = test_state(RelayConfig::default(), None);
        let app = admin_router(state.clone());
        let token = state.config.read().await.admin_token.clone();

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/config")
                    .header("authorization", bearer(&token))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"display_name":"renamed"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/config")
                    .header("authorization", bearer(&token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["display_name"], "renamed");
    }

    #[tokio::test]
    async fn put_config_rejects_invalid_and_preserves_state() {
        let state = test_state(RelayConfig::default(), None);
        let app = admin_router(state.clone());
        let token = state.config.read().await.admin_token.clone();
        let before = state.config.read().await.tiers.self_max_bytes;

        // self_max_bytes = 0 → validation failure
        let resp = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/config")
                    .header("authorization", bearer(&token))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"tiers":{"self_max_bytes":0}}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let after = state.config.read().await.tiers.self_max_bytes;
        assert_eq!(before, after, "config unchanged on invalid patch");
    }

    #[tokio::test]
    async fn put_config_persists_to_disk_when_path_set() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay.toml");
        let state = test_state(RelayConfig::default(), Some(path.clone()));
        let app = admin_router(state.clone());
        let token = state.config.read().await.admin_token.clone();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/config")
                    .header("authorization", bearer(&token))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"display_name":"persisted"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(path.exists(), "config file was created");

        let reloaded = RelayConfig::from_file(&path).unwrap();
        assert_eq!(reloaded.display_name, "persisted");
    }

    #[tokio::test]
    async fn put_config_updates_quota_manager() {
        let state = test_state(RelayConfig::default(), None);
        let app = admin_router(state.clone());
        let token = state.config.read().await.admin_token.clone();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/config")
                    .header("authorization", bearer(&token))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"quota":{"default_max_interfaces_per_peer":7}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        assert_eq!(
            state.quota.config_snapshot().default_max_interfaces_per_peer,
            7
        );
    }

    #[tokio::test]
    async fn put_config_unauthorized_without_token() {
        let state = test_state(RelayConfig::default(), None);
        let app = admin_router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/config")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"display_name":"x"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
