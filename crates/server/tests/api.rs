use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use nico_karaoke_core::{
    AppConfig, FakeMediaProvider, LaunchMode, Library, LookupQuery, LrclibTrack, LyricsClient,
    LyricsError, LyricsLookup,
};
use nico_karaoke_server::watcher::Watcher;
use nico_karaoke_server::{AppState, api, app};
use serde_json::Value;
use tower::ServiceExt;

struct NoopLyrics;

#[async_trait]
impl LyricsClient for NoopLyrics {
    async fn get(&self, _query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
        Ok(Vec::new())
    }

    async fn search(&self, _query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
        Ok(Vec::new())
    }
}

fn test_state() -> (AppState, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("temp directory");
    let library = Arc::new(Library::open(temp.path()).expect("library opens"));
    let lookup = Arc::new(LyricsLookup::new(Arc::new(NoopLyrics)));
    let provider = Arc::new(FakeMediaProvider::default());
    let watcher = Arc::new(Watcher::new(provider, lookup, library));

    (
        AppState {
            config: Arc::new(AppConfig {
                host: std::net::Ipv4Addr::LOCALHOST.into(),
                port: 8770,
                data_dir: PathBuf::from("test-data"),
                launch: LaunchMode::NoOpen,
            }),
            web_dir: Arc::new(PathBuf::from("web/dist")),
            watcher,
        },
        temp,
    )
}

#[tokio::test]
async fn health_returns_ok_json() {
    let (state, _temp) = test_state();
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body should be readable")
        .to_bytes();
    let payload: Value = serde_json::from_slice(&body).expect("health body should be JSON");

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["name"], "Nico Karaoke");
    assert!(payload["version"].is_string());
    assert!(payload["platform"].is_string());
    assert!(payload["supported"].is_boolean());
}

#[tokio::test]
async fn unknown_api_route_returns_json_404() {
    let (state, _temp) = test_state();
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/api/does-not-exist")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body should be readable")
        .to_bytes();
    let payload: Value = serde_json::from_slice(&body).expect("error body should be JSON");

    assert_eq!(payload["error"], "not_found");
}

#[tokio::test]
async fn bare_api_route_returns_json_404() {
    let (state, _temp) = test_state();
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/api")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body should be readable")
        .to_bytes();
    let payload: Value = serde_json::from_slice(&body).expect("error body should be JSON");

    assert_eq!(payload["error"], "not_found");
}

#[tokio::test]
async fn missing_static_file_returns_404() {
    // The app is multi-page: the Studio will be a real server route, not a client-side
    // catch-all. Unknown static paths should therefore not fall back to index.html.
    let (state, _temp) = test_state();
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/missing.js")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body should be readable")
        .to_bytes();
    assert!(!body.starts_with(b"<!doctype html>"));
}

#[tokio::test]
async fn security_headers_are_set_on_api_and_rejections() {
    let (state, _temp) = test_state();
    for uri in ["/api/health", "/api/does-not-exist"] {
        let response = app(state.clone())
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        let headers = response.headers();
        assert_eq!(
            headers
                .get("x-content-type-options")
                .and_then(|v| v.to_str().ok()),
            Some("nosniff")
        );
        assert_eq!(
            headers.get("x-frame-options").and_then(|v| v.to_str().ok()),
            Some("DENY")
        );
        assert_eq!(
            headers.get("referrer-policy").and_then(|v| v.to_str().ok()),
            Some("no-referrer")
        );
        assert_eq!(
            headers
                .get("cross-origin-opener-policy")
                .and_then(|v| v.to_str().ok()),
            Some("same-origin")
        );
        let csp = headers
            .get("content-security-policy")
            .and_then(|v| v.to_str().ok())
            .expect("CSP is set");
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("frame-ancestors 'none'"));
    }

    // Foreign requests are rejected by the outermost local-only middleware; hardening
    // headers must still be present.
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("host", "evil.example")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response
            .headers()
            .get("x-content-type-options")
            .and_then(|v| v.to_str().ok()),
        Some("nosniff")
    );
}

#[tokio::test]
async fn static_routes_carry_security_headers() {
    let (mut state, _temp) = test_state();
    let web = tempfile::tempdir().expect("web directory");
    std::fs::write(web.path().join("index.html"), "<!doctype html>ok").expect("index");
    std::fs::write(web.path().join("editor.html"), "<!doctype html>studio").expect("editor");
    state.web_dir = Arc::new(web.path().to_path_buf());

    for (uri, expected) in [
        ("/", StatusCode::OK),
        ("/editor", StatusCode::OK),
        ("/missing.js", StatusCode::NOT_FOUND),
    ] {
        let response = app(state.clone())
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("host", "127.0.0.1")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), expected, "{uri}");
        let headers = response.headers();
        assert_eq!(
            headers
                .get("x-content-type-options")
                .and_then(|v| v.to_str().ok()),
            Some("nosniff"),
            "{uri}"
        );
        assert!(
            headers
                .get("content-security-policy")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|value| value.contains("script-src 'self'")),
            "{uri} must carry the CSP"
        );
    }
}

#[tokio::test]
async fn static_serving_blocks_path_traversal() {
    let (mut state, _temp) = test_state();
    let root = tempfile::tempdir().expect("web root");
    let web = root.path().join("web");
    std::fs::create_dir(&web).expect("web directory");
    std::fs::write(web.join("index.html"), "<!doctype html>ok").expect("index");
    std::fs::write(root.path().join("secret.txt"), "TOP SECRET").expect("secret");
    state.web_dir = Arc::new(web);

    for uri in [
        "/%2e%2e/secret.txt",
        "/..%2fsecret.txt",
        "/../../secret.txt",
    ] {
        let response = app(state.clone())
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert!(
            response.status() != StatusCode::OK,
            "{uri} must not be served"
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body should be readable")
            .to_bytes();
        assert!(!body.starts_with(b"TOP SECRET"));
    }
}

#[tokio::test]
async fn oversized_json_body_is_rejected_with_a_code() {
    let (state, _temp) = test_state();
    let text = "a".repeat(api::MAX_BODY);
    let body = serde_json::json!({ "text": text }).to_string();
    let response = app(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/lyrics/parse")
                .header("host", "127.0.0.1")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body should be readable")
        .to_bytes();
    let payload: Value = serde_json::from_slice(&body).expect("error body should be JSON");
    assert_eq!(payload["error"], "payload_too_large");
}

#[tokio::test]
async fn editor_route_serves_the_studio_page() {
    let (mut state, _temp) = test_state();
    let web = tempfile::tempdir().expect("web directory");
    std::fs::write(
        web.path().join("editor.html"),
        "<!doctype html><title>studio fixture</title>",
    )
    .expect("editor page");
    state.web_dir = Arc::new(web.path().to_path_buf());

    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/editor")
                .header("host", "127.0.0.1")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body should be readable")
        .to_bytes();
    assert!(String::from_utf8_lossy(&body).contains("studio fixture"));
}

#[tokio::test]
async fn parse_lyrics_returns_lines_without_writing() {
    let (state, _temp) = test_state();
    let response = app(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/lyrics/parse")
                .header("host", "127.0.0.1")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"text":"[00:01.00]First [end:00:04.00]\n[00:06.00]Second\n"}"#,
                ))
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body should be readable")
        .to_bytes();
    let payload: Value = serde_json::from_slice(&body).expect("parse body should be JSON");

    assert_eq!(payload["synced"], true);
    assert_eq!(payload["lines"][0]["t"], 1.0);
    assert_eq!(payload["lines"][0]["end"], 4.0);
    assert_eq!(payload["lines"][1]["text"], "Second");
}

#[tokio::test]
async fn parse_lyrics_rejects_empty_text() {
    let (state, _temp) = test_state();
    let response = app(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/lyrics/parse")
                .header("host", "127.0.0.1")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"text":"   "}"#))
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body should be readable")
        .to_bytes();
    let payload: Value = serde_json::from_slice(&body).expect("error body should be JSON");
    assert_eq!(payload["error"], "lyrics_empty");
}
