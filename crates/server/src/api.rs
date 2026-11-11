use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use nico_karaoke_core::{APP_NAME, APP_VERSION};
use nico_karaoke_platform::{is_supported, target_label};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub platform: String,
    pub supported: bool,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        name: APP_NAME,
        version: APP_VERSION,
        platform: target_label(),
        supported: is_supported(),
    })
}

pub async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "not_found" })))
}
