//! Local HTTP server and static asset host for Nico Karaoke.

pub mod api;
pub mod dto;
pub mod logging;
pub mod lrclib_http;
pub mod runtime;
pub mod watcher;

mod embedded;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{StatusCode, Uri, header};
use axum::middleware;
use axum::response::{IntoResponse, Response};
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

/// Whether the frontend was embedded into this binary at build time.
pub fn has_embedded_assets() -> bool {
    embedded::is_available()
}

/// Build the application router. The web directory and watcher are injected so tests can use
/// fixtures without touching the process environment or a real player.
pub fn app(state: AppState) -> Router {
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

    let router = if state.web_dir.is_dir() {
        let static_files = ServeDir::new(state.web_dir.as_ref());
        let editor_page = ServeFile::new(state.web_dir.join("editor.html"));
        api.route_service("/editor", editor_page.clone())
            .route_service("/editor/", editor_page)
            .fallback_service(static_files)
    } else {
        api.route("/editor", get(embedded_asset))
            .route("/editor/", get(embedded_asset))
            .fallback(embedded_asset)
    };

    router
        .layer(DefaultBodyLimit::max(api::MAX_BODY))
        .layer(middleware::from_fn(api::no_store))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            api::local_only,
        ))
        .layer(middleware::from_fn(api::security_headers))
        .with_state(state)
}

async fn embedded_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = match path {
        "" => "index.html",
        "editor" | "editor/" => "editor.html",
        other => other,
    };
    let Some(bytes) = embedded::get(path) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    ([(header::CONTENT_TYPE, content_type(path))], bytes).into_response()
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::{content_type, has_embedded_assets};

    #[test]
    fn serves_common_asset_types() {
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            content_type("assets/app.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(content_type("assets/app.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("assets/app.png"), "image/png");
        assert_eq!(content_type("assets/idle-lime.webp"), "image/webp");
        assert_eq!(content_type("no-extension"), "application/octet-stream");
    }

    #[test]
    fn embeds_the_frontend_when_it_was_built() {
        if !has_embedded_assets() {
            return;
        }
        assert!(super::embedded::get("index.html").is_some());
        assert!(super::embedded::get("editor.html").is_some());
    }
}
