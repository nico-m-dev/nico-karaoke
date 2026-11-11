use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{HeaderValue, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use nico_karaoke_core::{
    AppConfig, FakeMediaProvider, LaunchMode, Library, LrclibTrack, LyricRecord, LyricsClient,
    LyricsError, LyricsLookup, PlayerSnapshot, record_key,
};
use nico_karaoke_server::watcher::{LyricsView, Watcher};
use nico_karaoke_server::{AppState, app};
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

struct FakeLyrics {
    responses: Mutex<VecDeque<Result<Vec<LrclibTrack>, LyricsError>>>,
}

impl FakeLyrics {
    fn new(responses: Vec<Result<Vec<LrclibTrack>, LyricsError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
        }
    }
}

#[async_trait]
impl LyricsClient for FakeLyrics {
    async fn get(
        &self,
        _query: &nico_karaoke_core::LookupQuery,
    ) -> Result<Vec<LrclibTrack>, LyricsError> {
        self.responses
            .lock()
            .expect("responses")
            .pop_front()
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    async fn search(
        &self,
        _query: &nico_karaoke_core::LookupQuery,
    ) -> Result<Vec<LrclibTrack>, LyricsError> {
        self.responses
            .lock()
            .expect("responses")
            .pop_front()
            .unwrap_or_else(|| Ok(Vec::new()))
    }
}

struct Fixture {
    state: AppState,
    _temp: TempDir,
    provider: Arc<FakeMediaProvider>,
    library: Arc<Library>,
    watcher: Arc<Watcher>,
}

impl Fixture {
    fn new(
        snapshots: Vec<PlayerSnapshot>,
        responses: Vec<Result<Vec<LrclibTrack>, LyricsError>>,
    ) -> Self {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Arc::new(Library::open(temp.path()).expect("library opens"));
        let lookup = Arc::new(LyricsLookup::new(Arc::new(FakeLyrics::new(responses))));
        let provider = Arc::new(FakeMediaProvider::new(snapshots));
        let watcher = Arc::new(Watcher::new(provider.clone(), lookup, library.clone()));
        let state = AppState {
            config: Arc::new(AppConfig {
                host: std::net::Ipv4Addr::LOCALHOST.into(),
                port: 8770,
                data_dir: PathBuf::from("test-data"),
                launch: LaunchMode::NoOpen,
            }),
            web_dir: Arc::new(PathBuf::from("web/dist")),
            watcher: watcher.clone(),
        };

        Self {
            state,
            _temp: temp,
            provider,
            library,
            watcher,
        }
    }

    async fn request(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.request_with_headers(method, uri, body, &[]).await
    }

    async fn request_with_headers(
        &self,
        method: Method,
        uri: &str,
        body: Option<Value>,
        headers: &[(header::HeaderName, HeaderValue)],
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        if !headers.iter().any(|(name, _)| *name == header::HOST) {
            builder = builder.header(header::HOST, "127.0.0.1");
        }
        let body = if let Some(body) = body {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(&body).expect("body serializes"))
        } else {
            Body::empty()
        };
        for (name, value) in headers {
            builder = builder.header(name, value);
        }

        let response = app(self.state.clone())
            .oneshot(builder.body(body).expect("request builds"))
            .await
            .expect("router responds");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body readable")
            .to_bytes();
        let payload = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, payload)
    }

    async fn wait_for_found(&self) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if matches!(self.watcher.state().lyrics, LyricsView::Found(_)) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("lookup finishes");
    }
}

fn snapshot(player: &str, title: &str, artist: &str) -> PlayerSnapshot {
    let mut snapshot = PlayerSnapshot::new(player, title, artist);
    snapshot.playing = true;
    snapshot.can_seek = true;
    snapshot.track_id = format!("/track/{player}");
    snapshot.length = Some(180.0);
    snapshot.position = Some(12.0);
    snapshot
}

fn lrclib_track(id: i64, title: &str, artist: &str, synced: &str) -> LrclibTrack {
    LrclibTrack {
        id: Some(id),
        track_name: title.to_owned(),
        artist_name: artist.to_owned(),
        album_name: String::new(),
        duration: Some(180.0),
        instrumental: false,
        plain_lyrics: None,
        synced_lyrics: Some(synced.to_owned()),
    }
}

#[tokio::test]
async fn now_returns_the_cached_library_record() {
    let fixture = Fixture::new(vec![snapshot("player", "Song", "Artist")], vec![]);
    let key = record_key("Song", "Artist");
    fixture
        .library
        .put(LyricRecord::new(
            &key,
            "Song",
            "Artist",
            "[00:01.00]First line\n",
        ))
        .expect("record saves");
    fixture.watcher.tick().await;

    let (status, body) = fixture.request(Method::GET, "/api/now", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["track"]["title"], "Song");
    assert_eq!(body["track"]["playing"], true);
    assert_eq!(body["lyrics"]["state"], "found");
    assert_eq!(body["lyrics"]["record_key"], key);
    assert_eq!(body["lyrics"]["lines"][0]["text"], "First line");
}

#[tokio::test]
async fn background_lookup_saves_found_lyrics() {
    let fixture = Fixture::new(
        vec![snapshot("player", "Song", "Artist")],
        vec![
            Ok(Vec::new()),
            Ok(vec![lrclib_track(
                7,
                "Song",
                "Artist",
                "[00:01.00]Fetched line\n",
            )]),
        ],
    );
    fixture.watcher.tick().await;
    fixture.wait_for_found().await;

    let key = record_key("Song", "Artist");
    assert!(fixture.library.get(&key).is_some());

    let (status, body) = fixture.request(Method::GET, "/api/now", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["lyrics"]["state"], "found");
    assert_eq!(body["lyrics"]["lines"][0]["text"], "Fetched line");
}

#[tokio::test]
async fn choosing_a_player_updates_track_and_control_target() {
    let mut first = snapshot("player-a", "Song A", "Artist");
    first.playing = true;
    let mut second = snapshot("player-b", "Song B", "Artist");
    second.playing = false;
    let fixture = Fixture::new(vec![first, second], vec![]);
    fixture.watcher.tick().await;

    let (status, _) = fixture
        .request(
            Method::POST,
            "/api/player",
            Some(json!({ "player": "player-b" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = fixture.request(Method::GET, "/api/now", None).await;
    assert_eq!(body["track"]["player"], "player-b");
    assert_eq!(body["track"]["title"], "Song B");

    let (status, _) = fixture
        .request(
            Method::POST,
            "/api/control",
            Some(json!({ "action": "playpause" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let actions = fixture.provider.actions();
    assert_eq!(actions[0].0, "player-b");
}

#[tokio::test]
async fn saving_current_lyrics_and_offset_updates_state() {
    let fixture = Fixture::new(vec![snapshot("player", "Song", "Artist")], vec![]);
    let key = record_key("Song", "Artist");
    fixture
        .library
        .put(LyricRecord::new(
            &key,
            "Song",
            "Artist",
            "[00:01.00]First\n",
        ))
        .expect("record saves");
    fixture.watcher.tick().await;

    let (status, body) = fixture
        .request(
            Method::POST,
            "/api/lyrics",
            Some(json!({
                "text": "[00:02.00]Edited\n",
                "source": "manual"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["lyrics"]["lines"][0]["text"], "Edited");
    assert_eq!(body["lyrics"]["state"], "found");

    let (status, body) = fixture
        .request(
            Method::POST,
            "/api/lyrics/offset",
            Some(json!({ "offset": 0.5 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["lyrics"]["offset"], 0.5);
}

#[tokio::test]
async fn library_create_list_read_delete_round_trip() {
    let fixture = Fixture::new(vec![], vec![]);
    fixture.watcher.tick().await;

    let (status, body) = fixture
        .request(
            Method::POST,
            "/api/library",
            Some(json!({
                "title": "Song",
                "artist": "Artist",
                "text": "[00:01.00]First\n"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let key = body["lyrics"]["record_key"]
        .as_str()
        .expect("record key")
        .to_owned();

    let (status, body) = fixture.request(Method::GET, "/api/library", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().expect("items").len(), 1);
    assert_eq!(body["items"][0]["key"], key);

    let (status, body) = fixture
        .request(Method::GET, &format!("/api/library/{key}"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["lyrics"]["record_key"], key);
    assert_eq!(body["lyrics"]["lines"][0]["text"], "First");

    let (status, _) = fixture
        .request(Method::DELETE, &format!("/api/library/{key}"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(fixture.library.get(&key).is_none());
}

#[tokio::test]
async fn fetch_returns_lyrics_without_saving_them() {
    let fixture = Fixture::new(
        vec![],
        vec![Ok(vec![lrclib_track(
            9,
            "Song",
            "Artist",
            "[00:01.00]Fetched\n",
        )])],
    );

    let (status, body) = fixture
        .request(
            Method::POST,
            "/api/lyrics/fetch",
            Some(json!({ "title": "Song", "artist": "Artist" })),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "found");
    assert!(fixture.library.get(&record_key("Song", "Artist")).is_none());
}

#[tokio::test]
async fn fetch_apply_replaces_a_manual_text_and_keeps_the_original() {
    let fixture = Fixture::new(
        vec![],
        vec![Ok(vec![lrclib_track(
            9,
            "Song",
            "Artist",
            "[00:01.00]Fetched\n",
        )])],
    );
    let key = record_key("Song", "Artist");

    let (status, _) = fixture
        .request(
            Method::POST,
            "/api/library",
            Some(json!({
                "title": "Song",
                "artist": "Artist",
                "text": "[00:01.00]Manual\n",
                "source": "manual"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = fixture
        .request(
            Method::POST,
            "/api/lyrics/fetch",
            Some(json!({ "item_key": key, "apply": true })),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["record_key"], key);
    assert_eq!(body["source"], "lrclib");
    assert!(body["original"].is_object());

    let stored = fixture.library.get(&key).expect("item is stored");
    assert!(stored.text.contains("Fetched"));
    assert!(!stored.text.contains("Manual"));
    assert!(stored.original.is_some());
}

#[tokio::test]
async fn stale_lookup_results_do_not_replace_the_new_track() {
    use std::sync::atomic::{AtomicBool, Ordering};

    use nico_karaoke_core::LookupQuery;

    struct StaleLyrics {
        started: tokio::sync::Notify,
        release: tokio::sync::Notify,
        blocked_once: AtomicBool,
    }

    #[async_trait]
    impl LyricsClient for StaleLyrics {
        async fn get(&self, query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
            if query.title == "Song A" && !self.blocked_once.swap(true, Ordering::SeqCst) {
                self.started.notify_one();
                self.release.notified().await;
            }
            if query.title == "Song A" {
                Ok(vec![lrclib_track(
                    1,
                    "Song A",
                    "Artist",
                    "[00:01.00]Old track\n",
                )])
            } else {
                Ok(vec![lrclib_track(
                    2,
                    "Song B",
                    "Artist",
                    "[00:01.00]New track\n",
                )])
            }
        }

        async fn search(&self, _query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
            Ok(Vec::new())
        }
    }

    let temp = tempfile::tempdir().expect("temp directory");
    let library = Arc::new(Library::open(temp.path()).expect("library opens"));
    let client = Arc::new(StaleLyrics {
        started: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
        blocked_once: AtomicBool::new(false),
    });
    let provider = Arc::new(FakeMediaProvider::new(vec![snapshot(
        "player", "Song A", "Artist",
    )]));
    let watcher = Arc::new(Watcher::new(
        provider.clone(),
        Arc::new(LyricsLookup::new(client.clone())),
        library,
    ));

    watcher.tick().await;
    client.started.notified().await;

    provider.set_snapshots(vec![snapshot("player", "Song B", "Artist")]);
    watcher.tick().await;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if matches!(watcher.state().lyrics, LyricsView::Found(_)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("new track lookup finishes");
    let LyricsView::Found(record) = watcher.state().lyrics else {
        panic!("expected new track");
    };
    assert_eq!(record.title, "Song B");

    client.release.notify_one();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let LyricsView::Found(record) = watcher.state().lyrics else {
        panic!("new track should stay");
    };
    assert_eq!(record.title, "Song B");
    drop(temp);
}

#[tokio::test]
async fn rejects_foreign_host_and_origin() {
    let fixture = Fixture::new(vec![], vec![]);

    let (status, body) = fixture
        .request_with_headers(
            Method::GET,
            "/api/now",
            None,
            &[(header::HOST, HeaderValue::from_static("evil.example"))],
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "forbidden_host");

    let (status, body) = fixture
        .request_with_headers(
            Method::GET,
            "/api/now",
            None,
            &[(
                header::ORIGIN,
                HeaderValue::from_static("https://evil.example"),
            )],
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "forbidden_host");
}

#[tokio::test]
async fn a_miss_is_scoped_to_the_track() {
    let fixture = Fixture::new(
        vec![snapshot("player", "Song A", "Artist")],
        vec![
            Ok(Vec::new()),
            Ok(Vec::new()),
            Ok(vec![lrclib_track(
                2,
                "Song B",
                "Artist",
                "[00:01.00]B line\n",
            )]),
        ],
    );
    fixture.watcher.tick().await;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if matches!(fixture.watcher.state().lyrics, LyricsView::Missing { .. }) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("first miss is recorded");

    fixture
        .provider
        .set_snapshots(vec![snapshot("player", "Song B", "Artist")]);
    fixture.watcher.tick().await;
    fixture.wait_for_found().await;

    let (status, body) = fixture.request(Method::GET, "/api/now", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["lyrics"]["state"], "found");
    assert_eq!(body["lyrics"]["lines"][0]["text"], "B line");
}

#[tokio::test]
async fn rejecting_foreign_host_is_no_store() {
    let fixture = Fixture::new(vec![], vec![]);
    let response = app(fixture.state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/now")
                .header(header::HOST, "evil.example")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
}

#[tokio::test]
async fn restore_requires_json_and_restores_the_original() {
    let fixture = Fixture::new(vec![], vec![]);
    let (status, body) = fixture
        .request(
            Method::POST,
            "/api/library",
            Some(json!({
                "title": "Song",
                "artist": "Artist",
                "text": "[00:02.00]Edited\n",
                "original_text": "[00:01.00]Original\n"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let key = body["lyrics"]["record_key"]
        .as_str()
        .expect("record key")
        .to_owned();

    let response = app(fixture.state.clone())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/library/{key}/original"))
                .header(header::HOST, "127.0.0.1")
                .header(header::ORIGIN, "null")
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from("x"))
                .expect("request builds"),
        )
        .await
        .expect("router responds");
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let (status, body) = fixture
        .request(
            Method::POST,
            &format!("/api/library/{key}/original"),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["lyrics"]["lines"][0]["text"], "Original");
}

#[tokio::test]
async fn rejects_wrong_content_type_and_invalid_json() {
    let fixture = Fixture::new(vec![], vec![]);

    let response = app(fixture.state.clone())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/player")
                .header(header::HOST, "127.0.0.1")
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from("player"))
                .expect("request builds"),
        )
        .await
        .expect("router responds");
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body readable")
        .to_bytes();
    let body: Value = serde_json::from_slice(&bytes).expect("error json");
    assert_eq!(body["error"], "unsupported_media_type");

    let (status, body) = fixture
        .request(Method::POST, "/api/player", Some(json!({ "player": 7 })))
        .await;
    assert!(status.is_client_error());
    assert_eq!(body["error"], "invalid_json");
}
