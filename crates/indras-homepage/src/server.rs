//! axum HTTP server setup and routes

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, extract::State, http::StatusCode, response::Html};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

use indras_artifacts::AccessGrant;
use indras_profile::{Profile, ViewLevel};
use crate::HomepageError;
use crate::auth::OptionalViewer;
use crate::grants;
use crate::templates;

/// Shared state for the axum server.
#[derive(Clone)]
pub struct AppState {
    pub profile: Arc<RwLock<Profile>>,
    pub grants: Arc<RwLock<Vec<AccessGrant>>>,
    pub steward: [u8; 32],
}

/// Build the axum router
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", axum::routing::get(homepage))
        .route("/health", axum::routing::get(health))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

impl AsRef<[u8; 32]> for AppState {
    fn as_ref(&self) -> &[u8; 32] {
        &self.steward
    }
}

/// GET / — render the profile homepage
async fn homepage(
    State(state): State<AppState>,
    viewer: OptionalViewer,
) -> Html<String> {
    let profile = state.profile.read().await;
    let grants = state.grants.read().await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let view_level = match viewer.0 {
        Some(viewer_id) => grants::resolve_view_level(&viewer_id, &state.steward, &grants, now),
        None => ViewLevel::Public,
    };
    Html(templates::render_profile(&profile, view_level))
}

/// GET /health — JSON health check
async fn health() -> (StatusCode, String) {
    (StatusCode::OK, templates::render_health())
}

/// Start serving on the given address
pub async fn serve(
    addr: SocketAddr,
    profile: Arc<RwLock<Profile>>,
    grants: Arc<RwLock<Vec<AccessGrant>>>,
    steward: [u8; 32],
) -> Result<(), HomepageError> {
    let state = AppState { profile, grants, steward };
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| HomepageError::Bind(e.to_string()))?;
    info!(%addr, "Homepage server listening");
    axum::serve(listener, app)
        .await
        .map_err(|e| HomepageError::Serve(e.to_string()))
}
