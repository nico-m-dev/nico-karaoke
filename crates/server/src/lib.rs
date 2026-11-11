//! Local HTTP server and static asset host for Nico Karaoke.

pub mod api;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::routing::{any, get};
use nico_karaoke_core::AppConfig;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub web_dir: Arc<PathBuf>,
}

/// Build the application router. The web directory is injected so tests can use a fixture
/// without touching the process environment.
pub fn app(state: AppState) -> Router {
    let static_files = ServeDir::new(state.web_dir.as_ref());

    Router::new()
        .route("/api/health", get(api::health))
        .route("/api", any(api::not_found))
        .route("/api/{*path}", any(api::not_found))
        .fallback_service(static_files)
        .with_state(state)
}
