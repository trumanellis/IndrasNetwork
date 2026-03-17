//! axum HTTP server setup and routes.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::Multipart;
use axum::{Router, extract::State, http::StatusCode, response::Html};
use tokio::sync::{RwLock, broadcast};
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::auth::{OptionalViewer, RequiredSteward};
use crate::grants;
use crate::storage::ArtifactStore;
use crate::templates;
use crate::{ContentArtifact, HomepageError, HomepageEvent, HomepageStore, ProfileFieldArtifact};

/// Shared state for the axum server.
#[derive(Clone)]
pub struct AppState {
    /// Grant-filtered profile fields.
    pub fields: Arc<RwLock<Vec<ProfileFieldArtifact>>>,
    /// Content artifact metadata list.
    pub artifacts: Arc<RwLock<Vec<ContentArtifact>>>,
    /// Steward (page owner) public key.
    pub steward: [u8; 32],
    /// Optional file-backed artifact store for upload/download.
    pub artifact_store: Option<Arc<ArtifactStore>>,
    /// Optional persistent store for profile data.
    pub store: Option<Arc<dyn HomepageStore>>,
    /// Broadcast channel for mutation events.
    pub events: broadcast::Sender<HomepageEvent>,
}

impl AsRef<[u8; 32]> for AppState {
    fn as_ref(&self) -> &[u8; 32] {
        &self.steward
    }
}

/// Build the axum router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", axum::routing::get(homepage))
        .route("/health", axum::routing::get(health))
        .route("/api/profile", axum::routing::get(api_profile))
        .route(
            "/api/profile/fields",
            axum::routing::put(api_update_fields),
        )
        .route("/api/artifacts", axum::routing::post(api_upload_artifact))
        .route(
            "/api/artifacts/{id}",
            axum::routing::get(api_download_artifact).delete(api_delete_artifact),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ---------------------------------------------------------------------------
// HTML routes
// ---------------------------------------------------------------------------

/// GET / — render the homepage with grant-filtered fields and artifacts.
async fn homepage(
    State(state): State<AppState>,
    viewer: OptionalViewer,
) -> Html<String> {
    let fields = state.fields.read().await;
    let artifacts = state.artifacts.read().await;
    let now = unix_now();

    let viewer_id = viewer.0.as_ref();
    let visible_fields = grants::visible_fields(viewer_id, &state.steward, &fields, now);
    let visible_artifacts = grants::visible_artifacts(viewer_id, &state.steward, &artifacts, now);

    Html(templates::render_homepage(&visible_fields, &visible_artifacts))
}

/// GET /health — JSON health check.
async fn health() -> (StatusCode, String) {
    (StatusCode::OK, templates::render_health())
}

// ---------------------------------------------------------------------------
// JSON API routes
// ---------------------------------------------------------------------------

/// GET /api/profile — JSON version of the homepage data (grant-filtered).
async fn api_profile(
    State(state): State<AppState>,
    viewer: OptionalViewer,
) -> axum::Json<serde_json::Value> {
    let fields = state.fields.read().await;
    let artifacts = state.artifacts.read().await;
    let now = unix_now();

    let viewer_id = viewer.0.as_ref();
    let visible_fields = grants::visible_fields(viewer_id, &state.steward, &fields, now);
    let visible_artifacts = grants::visible_artifacts(viewer_id, &state.steward, &artifacts, now);

    axum::Json(serde_json::json!({
        "steward": hex::encode(state.steward),
        "fields": visible_fields,
        "artifacts": visible_artifacts.iter().map(|a| serde_json::json!({
            "artifact_id": hex::encode(a.artifact_id),
            "name": a.name,
            "mime_type": a.mime_type,
            "size": a.size,
            "created_at": a.created_at,
        })).collect::<Vec<_>>(),
    }))
}

/// PUT /api/profile/fields — replace all profile fields (steward only).
async fn api_update_fields(
    State(state): State<AppState>,
    _steward: RequiredSteward,
    axum::Json(fields): axum::Json<Vec<ProfileFieldArtifact>>,
) -> StatusCode {
    // Persist to store before updating in-memory state
    if let Some(ref store) = state.store {
        if let Err(e) = store.save_profile(&fields) {
            tracing::warn!(error = %e, "Failed to persist profile fields");
        }
    }
    *state.fields.write().await = fields;
    let _ = state.events.send(HomepageEvent::FieldsUpdated);
    StatusCode::OK
}

/// POST /api/artifacts — upload an artifact via multipart form (steward only).
///
/// Expects multipart fields:
/// - `name` (text): human-readable artifact name
/// - `data` (file): the artifact bytes
async fn api_upload_artifact(
    State(state): State<AppState>,
    _steward: RequiredSteward,
    mut multipart: Multipart,
) -> Result<(StatusCode, axum::Json<serde_json::Value>), (StatusCode, String)> {
    let store = state
        .artifact_store
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "artifact storage not configured".to_string()))?;

    let mut name: Option<String> = None;
    let mut data: Option<Vec<u8>> = None;
    let mut mime_type: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "name" => {
                name = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
                );
            }
            "data" => {
                mime_type = field.content_type().map(|s| s.to_string());
                data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                        .to_vec(),
                );
            }
            _ => { /* skip unknown fields */ }
        }
    }

    let name = name.ok_or((StatusCode::BAD_REQUEST, "missing 'name' field".to_string()))?;
    let data = data.ok_or((StatusCode::BAD_REQUEST, "missing 'data' field".to_string()))?;

    // Derive a deterministic artifact id from name + data hash.
    let artifact_id: [u8; 32] = *blake3::hash(&data).as_bytes();

    let now = unix_now();
    let meta = crate::storage::ArtifactMeta {
        name: name.clone(),
        mime_type: mime_type.clone(),
        size: data.len() as u64,
        created_at: now,
    };

    store
        .store(&artifact_id, &data, &meta)
        .await
        .map_err(|e| (StatusCode::INSUFFICIENT_STORAGE, e.to_string()))?;

    // Add to in-memory artifacts list.
    let content = ContentArtifact {
        artifact_id,
        name,
        mime_type,
        size: data.len() as u64,
        created_at: now,
        grants: Vec::new(), // new artifacts start with no grants (steward-only)
    };
    state.artifacts.write().await.push(content);

    let _ = state.events.send(HomepageEvent::ArtifactChanged);

    Ok((
        StatusCode::CREATED,
        axum::Json(serde_json::json!({
            "artifact_id": hex::encode(artifact_id),
        })),
    ))
}

/// GET /api/artifacts/:id — download an artifact (grant-checked).
async fn api_download_artifact(
    State(state): State<AppState>,
    viewer: OptionalViewer,
    axum::extract::Path(id_hex): axum::extract::Path<String>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    use axum::response::IntoResponse;

    let store = state
        .artifact_store
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "artifact storage not configured".to_string()))?;

    let id = parse_hex_id(&id_hex)?;
    let now = unix_now();

    // Check grant access via the in-memory artifact list.
    let artifacts = state.artifacts.read().await;
    let artifact = artifacts
        .iter()
        .find(|a| a.artifact_id == id)
        .ok_or((StatusCode::NOT_FOUND, "artifact not found".to_string()))?;

    if !grants::can_view(viewer.0.as_ref(), &state.steward, &artifact.grants, now) {
        return Err((StatusCode::FORBIDDEN, "access denied".to_string()));
    }
    drop(artifacts);

    let data = store
        .load(&id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    let meta = store.load_meta(&id).await.ok();
    let content_type = meta
        .and_then(|m| m.mime_type)
        .unwrap_or_else(|| "application/octet-stream".to_string());

    Ok(([(axum::http::header::CONTENT_TYPE, content_type)], data).into_response())
}

/// DELETE /api/artifacts/:id — remove an artifact (steward only).
async fn api_delete_artifact(
    State(state): State<AppState>,
    _steward: RequiredSteward,
    axum::extract::Path(id_hex): axum::extract::Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let store = state
        .artifact_store
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "artifact storage not configured".to_string()))?;

    let id = parse_hex_id(&id_hex)?;

    store
        .delete(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Remove from in-memory list.
    state.artifacts.write().await.retain(|a| a.artifact_id != id);

    let _ = state.events.send(HomepageEvent::ArtifactChanged);

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a 32-byte hex-encoded id string.
fn parse_hex_id(s: &str) -> Result<[u8; 32], (StatusCode, String)> {
    let bytes = hex::decode(s).map_err(|_| (StatusCode::BAD_REQUEST, "invalid hex id".to_string()))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| (StatusCode::BAD_REQUEST, "id must be 32 bytes".to_string()))?;
    Ok(arr)
}

/// Current Unix timestamp in seconds.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Start serving on the given address.
pub async fn serve(
    addr: SocketAddr,
    fields: Arc<RwLock<Vec<ProfileFieldArtifact>>>,
    artifacts: Arc<RwLock<Vec<ContentArtifact>>>,
    steward: [u8; 32],
    artifact_store: Option<Arc<ArtifactStore>>,
    store: Option<Arc<dyn HomepageStore>>,
    events: broadcast::Sender<HomepageEvent>,
) -> Result<(), HomepageError> {
    let state = AppState {
        fields,
        artifacts,
        steward,
        artifact_store,
        store,
        events,
    };
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| HomepageError::Bind(e.to_string()))?;
    info!(%addr, "Homepage server listening");
    axum::serve(listener, app)
        .await
        .map_err(|e| HomepageError::Serve(e.to_string()))
}
