//! axum HTTP server setup and routes

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, extract::State, http::StatusCode, response::Html};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

use indras_artifacts::AccessGrant;
use crate::HomepageError;
// grants module is available for ViewLevel resolution when auth is added
#[allow(unused_imports)]
use crate::grants;
use crate::profile::{Profile, ViewLevel};
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

/// GET / — render the profile homepage
async fn homepage(State(state): State<AppState>) -> Html<String> {
    let profile = state.profile.read().await;
    // TODO: derive viewer identity from auth headers when auth is added
    // For now, all viewers see the public view
    let view_level = ViewLevel::Public;
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
