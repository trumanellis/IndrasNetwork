//! axum HTTP server setup and routes

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, extract::State, http::StatusCode, response::Html};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::HomepageError;
use crate::profile::Profile;
use crate::templates;

/// Shared state for the axum server
pub type SharedProfile = Arc<RwLock<Profile>>;

/// Build the axum router
pub fn router(profile: SharedProfile) -> Router {
    Router::new()
        .route("/", axum::routing::get(homepage))
        .route("/health", axum::routing::get(health))
        .layer(CorsLayer::permissive())
        .with_state(profile)
}

/// GET / — render the profile homepage
async fn homepage(State(profile): State<SharedProfile>) -> Html<String> {
    let profile = profile.read().await;
    Html(templates::render_profile(&profile))
}

/// GET /health — JSON health check
async fn health() -> (StatusCode, String) {
    (StatusCode::OK, templates::render_health())
}

/// Start serving on the given address
pub async fn serve(addr: SocketAddr, profile: SharedProfile) -> Result<(), HomepageError> {
    let app = router(profile);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| HomepageError::Bind(e.to_string()))?;
    info!(%addr, "Homepage server listening");
    axum::serve(listener, app)
        .await
        .map_err(|e| HomepageError::Serve(e.to_string()))
}
