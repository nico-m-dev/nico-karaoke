use std::time::Instant;

use axum::Json;
use axum::extract::{FromRequest, Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use nico_karaoke_core::{
    APP_NAME, APP_VERSION, AppConfig, LibraryError, LrcDocument, LyricsSource, MediaError,
    PlaybackControl, PlayerSnapshot,
};
use nico_karaoke_platform::{is_supported, target_label};
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::AppState;
use crate::dto::{
    ErrorDto, LibraryDto, LibraryItemResponse, LyricsDto, NowDto, OkResponse, ParsedLyricsDto,
};
use crate::watcher::{NewLibraryItem, WatcherError};

/// Enough for a maximum working text plus a maximum preserved original.
pub const MAX_BODY: usize = 512_000;
const MAX_LYRICS: usize = 120_000;
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

type ApiError = (StatusCode, Json<ErrorDto>);

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

pub async fn now(State(state): State<AppState>) -> Json<NowDto> {
    Json(NowDto::from_state(&state.watcher.state(), Instant::now()))
}

pub async fn library(State(state): State<AppState>) -> Json<LibraryDto> {
    let (items, current) = state.watcher.library_items();
    let signature = state
        .watcher
        .state()
        .media
        .snapshot
        .as_ref()
        .map(PlayerSnapshot::signature);
    Json(LibraryDto::new(items, current, signature))
}

pub async fn library_item(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<LibraryItemResponse>, ApiError> {
    let record = state
        .watcher
        .library_item(&key)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "lyrics_not_found"))?;
    let current = state.watcher.is_current_key(&key);
    Ok(Json(LibraryItemResponse {
        lyrics: LyricsDto::found(record, None),
        current,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateLibraryRequest {
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    pub text: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub original_text: Option<String>,
}

pub async fn create_library(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<CreateLibraryRequest>,
) -> Result<(StatusCode, Json<LibraryItemResponse>), ApiError> {
    validate_text(&body.text)?;
    let source = parse_source(body.source.as_deref())?;
    if let Some(duration) = body.duration {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(api_error(StatusCode::BAD_REQUEST, "invalid_duration"));
        }
    }

    let record = state
        .watcher
        .create_library_item(NewLibraryItem {
            title: body.title,
            artist: body.artist,
            album: body.album,
            text: body.text,
            source,
            duration: body.duration,
            original_text: body.original_text,
        })
        .await
        .map_err(watcher_error)?;
    let current = state.watcher.is_current_key(&record.key);
    Ok((
        StatusCode::CREATED,
        Json(LibraryItemResponse {
            lyrics: LyricsDto::found(record, None),
            current,
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct SaveLibraryRequest {
    pub text: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub original_text: Option<String>,
}

pub async fn save_library_item(
    State(state): State<AppState>,
    Path(key): Path<String>,
    ApiJson(body): ApiJson<SaveLibraryRequest>,
) -> Result<Json<LibraryItemResponse>, ApiError> {
    validate_text(&body.text)?;
    let source = parse_source(body.source.as_deref())?;
    let record = state
        .watcher
        .save_library_text(&key, body.text, source, body.original_text)
        .await
        .map_err(watcher_error)?;
    let current = state.watcher.is_current_key(&key);
    Ok(Json(LibraryItemResponse {
        lyrics: LyricsDto::found(record, None),
        current,
    }))
}

pub async fn delete_library_item(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<OkResponse>, ApiError> {
    let removed = state
        .watcher
        .delete_library_item(&key)
        .await
        .map_err(watcher_error)?;
    if !removed {
        return Err(api_error(StatusCode::NOT_FOUND, "lyrics_not_found"));
    }
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Debug, Deserialize)]
pub struct EmptyRequest {}

pub async fn restore_original(
    State(state): State<AppState>,
    Path(key): Path<String>,
    ApiJson(_body): ApiJson<EmptyRequest>,
) -> Result<Json<LibraryItemResponse>, ApiError> {
    let record = state
        .watcher
        .restore_original(&key)
        .await
        .map_err(watcher_error)?;
    let current = state.watcher.is_current_key(&key);
    Ok(Json(LibraryItemResponse {
        lyrics: LyricsDto::found(record, None),
        current,
    }))
}

#[derive(Debug, Deserialize)]
pub struct PlayerRequest {
    pub player: String,
}

pub async fn choose_player(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<PlayerRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    if !state.watcher.choose_player(&body.player).await {
        return Err(api_error(StatusCode::NOT_FOUND, "player_not_found"));
    }
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Debug, Deserialize)]
pub struct ControlRequest {
    pub action: String,
    #[serde(default)]
    pub delta: Option<f64>,
    #[serde(default)]
    pub position: Option<f64>,
}

pub async fn control(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<ControlRequest>,
) -> Result<Json<OkResponse>, ApiError> {
    let control = match body.action.as_str() {
        "playpause" => PlaybackControl::PlayPause,
        "play" => PlaybackControl::Play,
        "pause" => PlaybackControl::Pause,
        "seek" => {
            let delta = body
                .delta
                .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "missing_delta"))?;
            if !delta.is_finite() {
                return Err(api_error(StatusCode::BAD_REQUEST, "invalid_value"));
            }
            PlaybackControl::SeekRelative { delta }
        }
        "seek_to" => {
            let position = body
                .position
                .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "missing_position"))?;
            if !position.is_finite() {
                return Err(api_error(StatusCode::BAD_REQUEST, "invalid_value"));
            }
            PlaybackControl::SeekTo { position }
        }
        _ => return Err(api_error(StatusCode::BAD_REQUEST, "unknown_action")),
    };

    state.watcher.control(control).await.map_err(media_error)?;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Debug, Deserialize)]
pub struct SaveLyricsRequest {
    pub text: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub original_text: Option<String>,
    #[serde(default)]
    pub track_id: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
}

pub async fn save_current_lyrics(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<SaveLyricsRequest>,
) -> Result<Json<LibraryItemResponse>, ApiError> {
    validate_text(&body.text)?;
    let source = parse_source(body.source.as_deref())?;
    let record = state
        .watcher
        .save_current_text(
            body.text,
            source,
            body.original_text,
            body.track_id.as_deref(),
            body.signature.as_deref(),
        )
        .await
        .map_err(watcher_error)?;
    Ok(Json(LibraryItemResponse {
        lyrics: LyricsDto::found(record, None),
        current: true,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ParseLyricsRequest {
    pub text: String,
}

/// Parse a raw LRC draft for the Studio. Nothing is written; the response is the same line
/// shape the library returns so the editor can keep one document model.
pub async fn parse_lyrics(
    ApiJson(body): ApiJson<ParseLyricsRequest>,
) -> Result<Json<ParsedLyricsDto>, ApiError> {
    validate_text(&body.text)?;
    let document = LrcDocument::parse(&body.text);
    Ok(Json(ParsedLyricsDto {
        synced: document.synced,
        lines: document.lines,
        metadata: document.metadata,
        extras: document.extras,
    }))
}

#[derive(Debug, Deserialize)]
pub struct OffsetRequest {
    pub offset: f64,
    #[serde(default)]
    pub signature: Option<String>,
}

pub async fn set_offset(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<OffsetRequest>,
) -> Result<Json<LibraryItemResponse>, ApiError> {
    let record = state
        .watcher
        .set_offset(body.offset, body.signature.as_deref())
        .await
        .map_err(watcher_error)?;
    Ok(Json(LibraryItemResponse {
        lyrics: LyricsDto::found(record, None),
        current: true,
    }))
}

#[derive(Debug, Deserialize)]
pub struct FetchRequest {
    #[serde(default)]
    pub item_key: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub length: Option<f64>,
    #[serde(default)]
    pub track_id: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
    /// Store the found lyrics as the working text, even when the item has a manual edit.
    #[serde(default)]
    pub apply: bool,
}

pub async fn fetch_lyrics(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<FetchRequest>,
) -> Result<Json<LyricsDto>, ApiError> {
    let current = state.watcher.state();
    let snapshot = current.media.snapshot.clone();

    if let Some(expected) = body.track_id.as_deref() {
        if !expected.is_empty()
            && snapshot
                .as_ref()
                .is_none_or(|snapshot| snapshot.track_id != expected)
        {
            return Err(api_error(StatusCode::CONFLICT, "track_changed"));
        }
    }
    if let Some(expected) = body.signature.as_deref() {
        if !expected.is_empty()
            && snapshot
                .as_ref()
                .is_none_or(|snapshot| snapshot.signature() != expected)
        {
            return Err(api_error(StatusCode::CONFLICT, "track_changed"));
        }
    }

    let item = body
        .item_key
        .as_deref()
        .filter(|key| !key.is_empty())
        .map(|key| {
            state
                .watcher
                .library_item(key)
                .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "lyrics_not_found"))
        })
        .transpose()?;

    let title = body
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| item.as_ref().map(|item| item.title.clone()))
        .or_else(|| snapshot.as_ref().map(|snapshot| snapshot.title.clone()))
        .ok_or_else(|| api_error(StatusCode::CONFLICT, "no_current_track"))?;
    let artist = body
        .artist
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| item.as_ref().map(|item| item.artist.clone()))
        .or_else(|| snapshot.as_ref().map(|snapshot| snapshot.artist.clone()))
        .unwrap_or_default();
    let album = body
        .album
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| item.as_ref().map(|item| item.album.clone()))
        .or_else(|| snapshot.as_ref().map(|snapshot| snapshot.album.clone()))
        .unwrap_or_default();
    let length = body
        .length
        .or_else(|| item.as_ref().and_then(|item| item.duration))
        .or_else(|| snapshot.as_ref().and_then(|snapshot| snapshot.length));
    if length.is_some_and(|value| !value.is_finite() || value <= 0.0) {
        return Err(api_error(StatusCode::BAD_REQUEST, "invalid_duration"));
    }
    if title.trim().is_empty() {
        return Err(api_error(StatusCode::CONFLICT, "no_current_track"));
    }

    let mut record = state
        .watcher
        .fetch(&title, &artist, &album, length)
        .await
        .map_err(watcher_error)?;
    if body.apply {
        record = state
            .watcher
            .save_fetched(record)
            .await
            .map_err(watcher_error)?;
    }
    let signature = if body.item_key.is_some() {
        None
    } else {
        snapshot.as_ref().map(PlayerSnapshot::signature)
    };
    Ok(Json(LyricsDto::found(record, signature)))
}

pub async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Json(ErrorDto::new("not_found")))
}

pub async fn local_only(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !is_local_request(&state.config, request.headers()) {
        return no_store_response(
            api_error(StatusCode::FORBIDDEN, "forbidden_host").into_response(),
        );
    }
    next.run(request).await
}

pub async fn no_store(request: Request, next: Next) -> Response {
    no_store_response(next.run(request).await)
}

/// Browser-side hardening for the local app. The CSP keeps the page self-contained: no remote
/// scripts, styles, fonts, or connections. Inline styles stay allowed because the player and
/// timeline position elements through CSS custom properties.
pub async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        header::HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        header::HeaderName::from_static("cross-origin-opener-policy"),
        HeaderValue::from_static("same-origin"),
    );
    response
}

fn no_store_response(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(request, state)
            .await
            .map_err(|rejection| {
                let status = rejection.status();
                let code = if status == StatusCode::UNSUPPORTED_MEDIA_TYPE {
                    "unsupported_media_type"
                } else if status == StatusCode::PAYLOAD_TOO_LARGE {
                    "payload_too_large"
                } else {
                    "invalid_json"
                };
                api_error(status, code)
            })?;
        Ok(Self(value))
    }
}

fn is_local_request(config: &AppConfig, headers: &HeaderMap) -> bool {
    // Fail closed: the Host header is the DNS-rebinding guard, so a missing or empty header
    // is treated as a foreign request instead of skipping validation.
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let name = host_name(host);
    if name.is_empty() || !allowed_host(config, &name) {
        return false;
    }

    if let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        if origin != "null" {
            let Ok(uri) = origin.parse::<axum::http::Uri>() else {
                return false;
            };
            if let Some(host) = uri.host() {
                let host = host
                    .strip_prefix('[')
                    .and_then(|value| value.strip_suffix(']'))
                    .unwrap_or(host)
                    .to_ascii_lowercase();
                if !allowed_host(config, &host) {
                    return false;
                }
            }
        }
    }
    true
}

fn allowed_host(config: &AppConfig, host: &str) -> bool {
    if matches!(host, "127.0.0.1" | "localhost" | "::1") {
        return true;
    }
    if config.host.is_unspecified() {
        return false;
    }
    host == config.host.to_string()
}

fn host_name(host: &str) -> String {
    let host = host.trim();
    if let Some(rest) = host.strip_prefix('[') {
        if let Some((inside, after)) = rest.split_once(']') {
            if after.is_empty() || after.starts_with(':') {
                return inside.to_ascii_lowercase();
            }
        }
    }
    host.rsplit_once(':')
        .map(|(name, _)| name)
        .unwrap_or(host)
        .to_ascii_lowercase()
}

fn validate_text(text: &str) -> Result<(), ApiError> {
    if text.trim().is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "lyrics_empty"));
    }
    if text.len() > MAX_LYRICS {
        return Err(api_error(StatusCode::PAYLOAD_TOO_LARGE, "lyrics_too_long"));
    }
    Ok(())
}

fn parse_source(value: Option<&str>) -> Result<LyricsSource, ApiError> {
    match value.unwrap_or("manual").to_ascii_lowercase().as_str() {
        "lrclib" => Ok(LyricsSource::Lrclib),
        "manual" => Ok(LyricsSource::Manual),
        "tap" => Ok(LyricsSource::Tap),
        _ => Err(api_error(StatusCode::BAD_REQUEST, "invalid_source")),
    }
}

fn api_error(status: StatusCode, error: &'static str) -> ApiError {
    (status, Json(ErrorDto::new(error)))
}

fn watcher_error(error: WatcherError) -> ApiError {
    match error {
        WatcherError::NoCurrentTrack => api_error(StatusCode::CONFLICT, "no_current_track"),
        WatcherError::TrackChanged => api_error(StatusCode::CONFLICT, "track_changed"),
        WatcherError::LyricsNotFound => api_error(StatusCode::NOT_FOUND, "lyrics_not_found"),
        WatcherError::LyricsPending => api_error(StatusCode::SERVICE_UNAVAILABLE, "lyrics_pending"),
        WatcherError::NoOriginal => api_error(StatusCode::NOT_FOUND, "no_original"),
        WatcherError::InvalidValue => api_error(StatusCode::BAD_REQUEST, "invalid_value"),
        WatcherError::Library(error) => {
            tracing::warn!(error = %error, "library operation failed");
            match error {
                LibraryError::EmptyText => api_error(StatusCode::BAD_REQUEST, "lyrics_empty"),
                LibraryError::MissingKey | LibraryError::InvalidKey => {
                    api_error(StatusCode::BAD_REQUEST, "invalid_key")
                }
                _ => api_error(StatusCode::INTERNAL_SERVER_ERROR, "library_error"),
            }
        }
        WatcherError::Task(error) => {
            tracing::error!(error = %error, "library task failed");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "library_error")
        }
        WatcherError::Lyrics(error) => {
            tracing::warn!(error = %error, "lyrics lookup failed");
            api_error(StatusCode::BAD_GATEWAY, "lyrics_unavailable")
        }
    }
}

fn media_error(error: MediaError) -> ApiError {
    match error {
        MediaError::NoPlayer => api_error(StatusCode::CONFLICT, "no_current_track"),
        MediaError::Unavailable(_) => {
            api_error(StatusCode::SERVICE_UNAVAILABLE, "media_unavailable")
        }
        MediaError::PlayerNotFound(_) => api_error(StatusCode::NOT_FOUND, "player_not_found"),
        MediaError::NotSupported(_) => api_error(StatusCode::CONFLICT, "not_supported"),
        MediaError::InvalidValue(_) => api_error(StatusCode::BAD_REQUEST, "invalid_value"),
        MediaError::Backend(_) => api_error(StatusCode::BAD_GATEWAY, "player_error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_bracketed_ipv6_hosts() {
        assert_eq!(host_name("[::1]:8770"), "::1");
        assert_eq!(host_name("localhost:8770"), "localhost");
        assert_eq!(host_name("127.0.0.1"), "127.0.0.1");
    }
}
