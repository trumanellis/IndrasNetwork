//! HTTP admin API for monitoring and management
//!
//! Provides health checks, statistics, and peer management
//! endpoints via axum with bearer token authentication.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use indras_core::identity::PeerIdentity;

use crate::auth::AuthService;
use crate::blob_store::BlobStore;
use crate::config::RelayConfig;
use crate::registration::RegistrationState;

/// Shared state for admin API handlers
pub struct AdminState {
    pub config: RelayConfig,
    pub blob_store: Arc<BlobStore>,
    pub registrations: Arc<RegistrationState>,
    pub auth: Arc<AuthService>,
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

/// Build the admin API router
pub fn admin_router(state: Arc<AdminState>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/stats", get(stats_handler))
        .route("/peers", get(peers_handler))
        .route("/interfaces", get(interfaces_handler))
        .route("/contacts", get(contacts_handler).put(update_contacts_handler))
        .with_state(state)
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
    if token != expected {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

/// GET /health — status and uptime (no auth required)
async fn health_handler(State(state): State<Arc<AdminState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        uptime_secs: state.started_at.elapsed().as_secs(),
        display_name: state.config.display_name.clone(),
    })
}

/// GET /stats — peer count, interface count, storage usage
async fn stats_handler(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<StatsResponse>, StatusCode> {
    validate_token(&headers, &state.config.admin_token)?;

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
    validate_token(&headers, &state.config.admin_token)?;

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
    validate_token(&headers, &state.config.admin_token)?;

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
    validate_token(&headers, &state.config.admin_token)?;

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
    validate_token(&headers, &state.config.admin_token)?;

    let parsed: Vec<[u8; 32]> = body
        .contacts
        .iter()
        .filter_map(|h| parse_hex_32(h))
        .collect();

    let count = parsed.len();
    state.auth.sync_contacts(parsed);

    let contacts_path = state.config.data_dir.join("contacts.json");
    state
        .auth
        .save_contacts(&contacts_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let contacts: Vec<String> = state.auth.contact_ids().iter().map(|id| hex::encode(id)).collect();

    Ok(Json(ContactsResponse { contacts, count }))
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
