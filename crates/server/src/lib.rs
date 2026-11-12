//! Local HTTP server and static asset host for Nico Karaoke.

pub mod api;
pub mod dto;
pub mod logging;
pub mod lrclib_http;
pub mod runtime;
pub mod watcher;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{any, get, post};
use nico_karaoke_core::AppConfig;
use tower_http::services::{ServeDir, ServeFile};

use crate::watcher::Watcher;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub web_dir: Arc<PathBuf>,
    pub watcher: Arc<Watcher>,
}

/// Build the application router. The web directory and watcher are injected so tests can use
/// fixtures without touching the process environment or a real player.
pub fn app(state: AppState) -> Router {
    let static_files = ServeDir::new(state.web_dir.as_ref());
    let editor_page = ServeFile::new(state.web_dir.join("editor.html"));
    let api = Router::new()
        .route("/api/health", get(api::health))
        .route("/api/now", get(api::now))
        .route("/api/library", get(api::library).post(api::create_library))
        .route(
            "/api/library/{key}",
            get(api::library_item)
                .put(api::save_library_item)
                .delete(api::delete_library_item),
        )
        .route("/api/library/{key}/original", post(api::restore_original))
        .route("/api/player", post(api::choose_player))
        .route("/api/control", post(api::control))
        .route("/api/lyrics", post(api::save_current_lyrics))
        .route("/api/lyrics/parse", post(api::parse_lyrics))
        .route("/api/lyrics/offset", post(api::set_offset))
        .route("/api/lyrics/fetch", post(api::fetch_lyrics))
        .route("/api", any(api::not_found))
        .route("/api/{*path}", any(api::not_found));

    api.route_service("/editor", editor_page.clone())
        .route_service("/editor/", editor_page)
        .fallback_service(static_files)
        .layer(DefaultBodyLimit::max(api::MAX_BODY))
        .layer(middleware::from_fn(api::no_store))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            api::local_only,
        ))
        .layer(middleware::from_fn(api::security_headers))
        .with_state(state)
}
