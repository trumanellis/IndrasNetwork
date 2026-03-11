//! axum HTTP server setup and routes

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, extract::State, http::StatusCode, response::Html};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::auth::OptionalViewer;
use crate::grants;
use crate::templates;
use crate::{ContentArtifact, HomepageError, ProfileFieldArtifact};

/// Shared state for the axum server.
#[derive(Clone)]
pub struct AppState {
    pub fields: Arc<RwLock<Vec<ProfileFieldArtifact>>>,
    pub artifacts: Arc<RwLock<Vec<ContentArtifact>>>,
    pub steward: [u8; 32],
}

impl AsRef<[u8; 32]> for AppState {
    fn as_ref(&self) -> &[u8; 32] {
        &self.steward
    }
}

/// Build the axum router
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", axum::routing::get(homepage))
        .route("/health", axum::routing::get(health))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// GET / — render the homepage with grant-filtered fields and artifacts
async fn homepage(
    State(state): State<AppState>,
    viewer: OptionalViewer,
) -> Html<String> {
    let fields = state.fields.read().await;
    let artifacts = state.artifacts.read().await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let viewer_id = viewer.0.as_ref();
    let visible_fields = grants::visible_fields(viewer_id, &state.steward, &fields, now);
    let visible_artifacts = grants::visible_artifacts(viewer_id, &state.steward, &artifacts, now);

    Html(templates::render_homepage(&visible_fields, &visible_artifacts))
}

/// GET /health — JSON health check
async fn health() -> (StatusCode, String) {
    (StatusCode::OK, templates::render_health())
}

/// Start serving on the given address
pub async fn serve(
    addr: SocketAddr,
    fields: Arc<RwLock<Vec<ProfileFieldArtifact>>>,
    artifacts: Arc<RwLock<Vec<ContentArtifact>>>,
    steward: [u8; 32],
) -> Result<(), HomepageError> {
    let state = AppState { fields, artifacts, steward };
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| HomepageError::Bind(e.to_string()))?;
    info!(%addr, "Homepage server listening");
    axum::serve(listener, app)
        .await
        .map_err(|e| HomepageError::Serve(e.to_string()))
}
