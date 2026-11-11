use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::library::{LIBRARY_VERSION, LyricRecord, LyricsSource, OriginalLyrics, normalize_text};
use crate::lyrics::parse_lrc;
use crate::title::{record_key, split_title};

pub const LRCLIB_BASE_URL: &str = "https://lrclib.net/api";
pub const MISS_TTL_SECONDS: f64 = 3600.0;
pub const DURATION_TOLERANCE_SECONDS: f64 = 5.0;

/// One track returned by LRCLIB. Unknown fields are ignored so the API can grow without
/// breaking the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LrclibTrack {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(rename = "trackName", default)]
    pub track_name: String,
    #[serde(rename = "artistName", default)]
    pub artist_name: String,
    #[serde(rename = "albumName", default)]
    pub album_name: String,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub instrumental: bool,
    #[serde(rename = "plainLyrics", default)]
    pub plain_lyrics: Option<String>,
    #[serde(rename = "syncedLyrics", default)]
    pub synced_lyrics: Option<String>,
}

impl LrclibTrack {
    pub fn has_synced(&self) -> bool {
        self.synced_lyrics
            .as_deref()
            .is_some_and(|text| !text.trim().is_empty())
    }

    pub fn usable(&self) -> bool {
        self.has_synced()
            || self
                .plain_lyrics
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty())
    }

    /// Prefer synchronised lyrics; fall back to plain text.
    pub fn lyrics_text(&self) -> String {
        self.synced_lyrics
            .clone()
            .filter(|text| !text.trim().is_empty())
            .or_else(|| {
                self.plain_lyrics
                    .clone()
                    .filter(|text| !text.trim().is_empty())
            })
            .unwrap_or_default()
    }
}

/// Cleaned query parameters for LRCLIB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupQuery {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<u64>,
}

impl LookupQuery {
    pub fn new(title: &str, artist: &str, album: &str, duration: Option<f64>) -> Option<Self> {
        let (title, artist) = split_title(title, artist);
        if title.trim().is_empty() {
            return None;
        }

        let duration = duration
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| value.round() as u64)
            .filter(|value| *value > 0);

        Some(Self {
            title,
            artist,
            album: album.trim().to_owned(),
            duration,
        })
    }

    pub fn without_album(&self) -> Self {
        Self {
            album: String::new(),
            ..self.clone()
        }
    }

    fn params(&self) -> Vec<(&'static str, String)> {
        let mut params = vec![
            ("track_name", self.title.clone()),
            ("artist_name", self.artist.clone()),
        ];
        if !self.album.is_empty() {
            params.push(("album_name", self.album.clone()));
        }
        if let Some(duration) = self.duration {
            params.push(("duration", duration.to_string()));
        }
        params
    }
}

/// Metadata of the track currently being followed.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackQuery {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<f64>,
}

impl TrackQuery {
    pub fn new(
        title: impl Into<String>,
        artist: impl Into<String>,
        album: impl Into<String>,
        duration: Option<f64>,
    ) -> Self {
        Self {
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            duration,
        }
    }
}

#[async_trait]
pub trait LyricsClient: Send + Sync {
    async fn get(&self, query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError>;
    async fn search(&self, query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError>;
}

#[derive(Debug, Error)]
pub enum LyricsError {
    #[error("LRCLIB request failed: {0}")]
    Request(String),
    #[error("LRCLIB returned invalid data: {0}")]
    Decode(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LookupOutcome {
    Found(Box<LyricRecord>),
    Missing {
        fetched_at: f64,
    },
    /// Another lookup for the same track is already running. The caller should keep showing
    /// its loading state and poll again later.
    Pending,
}

pub struct LyricsLookup {
    client: Arc<dyn LyricsClient>,
    misses: RwLock<HashMap<String, f64>>,
    in_flight: RwLock<HashSet<String>>,
    miss_ttl_seconds: f64,
}

impl LyricsLookup {
    pub fn new(client: Arc<dyn LyricsClient>) -> Self {
        Self::with_miss_ttl(client, MISS_TTL_SECONDS)
    }

    pub fn with_miss_ttl(client: Arc<dyn LyricsClient>, miss_ttl_seconds: f64) -> Self {
        Self {
            client,
            misses: RwLock::new(HashMap::new()),
            in_flight: RwLock::new(HashSet::new()),
            miss_ttl_seconds: miss_ttl_seconds.max(0.0),
        }
    }

    pub async fn lookup(&self, track: &TrackQuery) -> Result<LookupOutcome, LyricsError> {
        self.lookup_at(track, unix_now()).await
    }

    /// Lookup with an explicit clock for deterministic cache tests.
    pub async fn lookup_at(
        &self,
        track: &TrackQuery,
        now: f64,
    ) -> Result<LookupOutcome, LyricsError> {
        let key = record_key(&track.title, &track.artist);
        if let Some(fetched_at) = self.cached_miss(&key, now) {
            return Ok(LookupOutcome::Missing { fetched_at });
        }

        let Some(base) =
            LookupQuery::new(&track.title, &track.artist, &track.album, track.duration)
        else {
            return Ok(LookupOutcome::Missing { fetched_at: now });
        };

        if !self.begin_lookup(&key) {
            return Ok(LookupOutcome::Pending);
        }
        let result = self.lookup_query(&key, &base, track, now).await;
        self.end_lookup(&key);
        result
    }

    async fn lookup_query(
        &self,
        key: &str,
        base: &LookupQuery,
        track: &TrackQuery,
        now: f64,
    ) -> Result<LookupOutcome, LyricsError> {
        let without_album = base.without_album();

        let mut exact = Vec::new();
        let mut searched = Vec::new();
        let mut seen = HashSet::new();
        let mut errors = Vec::new();
        let mut successes = 0;

        if !base.album.is_empty() {
            match self.client.get(base).await {
                Ok(items) => {
                    successes += 1;
                    add_candidates(&mut exact, items, &mut seen);
                }
                Err(error) => errors.push(error),
            }
        }

        match self.client.get(&without_album).await {
            Ok(items) => {
                successes += 1;
                add_candidates(&mut exact, items, &mut seen);
            }
            Err(error) => errors.push(error),
        }

        if !exact.iter().any(LrclibTrack::has_synced) {
            match self.client.search(&without_album).await {
                Ok(items) => {
                    successes += 1;
                    add_candidates(&mut searched, items, &mut seen);
                }
                Err(error) => errors.push(error),
            }
        }

        if let Some(item) = select(&exact, &searched, track.duration) {
            return Ok(LookupOutcome::Found(Box::new(record_from(
                item, track, now,
            ))));
        }

        // Do not turn a temporary network failure into an hour-long miss.
        if successes == 0 {
            if let Some(error) = errors.into_iter().next() {
                return Err(error);
            }
        }

        self.remember_miss(key.to_owned(), now);
        Ok(LookupOutcome::Missing { fetched_at: now })
    }

    fn begin_lookup(&self, key: &str) -> bool {
        self.in_flight
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key.to_owned())
    }

    fn end_lookup(&self, key: &str) {
        self.in_flight
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(key);
    }

    fn cached_miss(&self, key: &str, now: f64) -> Option<f64> {
        let mut misses = self
            .misses
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match misses.get(key).copied() {
            Some(fetched_at) if now - fetched_at <= self.miss_ttl_seconds => Some(fetched_at),
            Some(_) => {
                misses.remove(key);
                None
            }
            None => None,
        }
    }

    fn remember_miss(&self, key: String, now: f64) {
        self.misses
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key, now);
    }
}

fn add_candidates(
    bucket: &mut Vec<LrclibTrack>,
    items: Vec<LrclibTrack>,
    seen: &mut HashSet<String>,
) {
    for item in items {
        if !item.usable() {
            continue;
        }
        let identity = item.id.map_or_else(
            || {
                format!(
                    "meta:{}|{}|{}",
                    item.track_name,
                    item.artist_name,
                    item.duration.unwrap_or_default()
                )
            },
            |id| format!("id:{id}"),
        );
        if seen.insert(identity) {
            bucket.push(item);
        }
    }
}

fn distance(item: &LrclibTrack, duration: Option<f64>) -> f64 {
    let Some(duration) = duration else {
        return 0.0;
    };
    let Some(item_duration) = item.duration else {
        return f64::INFINITY;
    };
    (item_duration - duration).abs()
}

fn near(item: &LrclibTrack, duration: Option<f64>) -> bool {
    duration.is_none() || distance(item, duration) <= DURATION_TOLERANCE_SECONDS
}

fn select<'a>(
    exact: &'a [LrclibTrack],
    searched: &'a [LrclibTrack],
    duration: Option<f64>,
) -> Option<&'a LrclibTrack> {
    if let Some(item) = exact.iter().find(|item| item.has_synced()) {
        return Some(item);
    }

    let mut synced_near: Vec<&LrclibTrack> = searched
        .iter()
        .filter(|item| item.has_synced() && near(item, duration))
        .collect();
    synced_near
        .sort_by(|left, right| distance(left, duration).total_cmp(&distance(right, duration)));
    if let Some(item) = synced_near.first() {
        return Some(item);
    }

    if let Some(item) = exact.first() {
        return Some(item);
    }

    let mut usable_near: Vec<&LrclibTrack> = searched
        .iter()
        .filter(|item| near(item, duration))
        .collect();
    usable_near.sort_by(|left, right| {
        let left_synced = u8::from(!left.has_synced());
        let right_synced = u8::from(!right.has_synced());
        left_synced
            .cmp(&right_synced)
            .then_with(|| distance(left, duration).total_cmp(&distance(right, duration)))
    });
    usable_near.first().copied()
}

fn record_from(item: &LrclibTrack, track: &TrackQuery, now: f64) -> LyricRecord {
    let text = normalize_text(&item.lyrics_text());
    let document = parse_lrc(&text);
    let original = if text.trim().is_empty() {
        None
    } else {
        Some(OriginalLyrics {
            text: text.clone(),
            source: LyricsSource::Lrclib,
            fetched_at: now,
            lrclib_id: item.id,
            synced: document.synced,
        })
    };

    LyricRecord {
        version: LIBRARY_VERSION,
        key: record_key(&track.title, &track.artist),
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        duration: track
            .duration
            .filter(|duration| duration.is_finite() && *duration > 0.0),
        source: LyricsSource::Lrclib,
        synced: document.synced,
        offset: 0.0,
        text,
        lines: document.lines,
        lrclib_id: item.id,
        fetched_at: now,
        updated_at: now,
        original,
    }
}

pub struct HttpLyricsClient {
    base: String,
    http: reqwest::Client,
}

impl HttpLyricsClient {
    pub fn new() -> Result<Self, LyricsError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(format!(
                "nico-karaoke/{} (local, personal use)",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|error| LyricsError::Request(error.to_string()))?;
        Ok(Self::with_client(LRCLIB_BASE_URL, http))
    }

    pub fn with_client(base: impl Into<String>, http: reqwest::Client) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            http,
        }
    }

    async fn request(
        &self,
        endpoint: &str,
        query: &LookupQuery,
    ) -> Result<reqwest::Response, LyricsError> {
        self.http
            .get(format!("{}/{endpoint}", self.base))
            .query(&query.params())
            .send()
            .await
            .map_err(|error| LyricsError::Request(error.to_string()))
    }
}

#[async_trait]
impl LyricsClient for HttpLyricsClient {
    async fn get(&self, query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
        let response = self.request("get", query).await?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| LyricsError::Request(error.to_string()))?;
        parse_get_json(status, &body)
    }

    async fn search(&self, query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
        let response = self.request("search", query).await?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| LyricsError::Request(error.to_string()))?;
        parse_search_json(status, &body)
    }
}

fn parse_get_json(
    status: reqwest::StatusCode,
    body: &str,
) -> Result<Vec<LrclibTrack>, LyricsError> {
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    if !status.is_success() {
        return Err(LyricsError::Request(format!(
            "LRCLIB get returned HTTP {status}"
        )));
    }
    let track: LrclibTrack =
        serde_json::from_str(body).map_err(|error| LyricsError::Decode(error.to_string()))?;
    Ok(vec![track])
}

fn parse_search_json(
    status: reqwest::StatusCode,
    body: &str,
) -> Result<Vec<LrclibTrack>, LyricsError> {
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    if !status.is_success() {
        return Err(LyricsError::Request(format!(
            "LRCLIB search returned HTTP {status}"
        )));
    }
    serde_json::from_str(body).map_err(|error| LyricsError::Decode(error.to_string()))
}

fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;

    fn track(
        id: i64,
        title: &str,
        artist: &str,
        duration: f64,
        synced: Option<&str>,
        plain: Option<&str>,
    ) -> LrclibTrack {
        LrclibTrack {
            id: Some(id),
            track_name: title.to_owned(),
            artist_name: artist.to_owned(),
            album_name: "Album".to_owned(),
            duration: Some(duration),
            instrumental: false,
            plain_lyrics: plain.map(str::to_owned),
            synced_lyrics: synced.map(str::to_owned),
        }
    }

    struct FakeClient {
        gets: Mutex<VecDeque<Result<Vec<LrclibTrack>, LyricsError>>>,
        searches: Mutex<VecDeque<Result<Vec<LrclibTrack>, LyricsError>>>,
        get_queries: Mutex<Vec<LookupQuery>>,
        search_queries: Mutex<Vec<LookupQuery>>,
    }

    impl FakeClient {
        fn new(
            gets: Vec<Result<Vec<LrclibTrack>, LyricsError>>,
            searches: Vec<Result<Vec<LrclibTrack>, LyricsError>>,
        ) -> Arc<Self> {
            Arc::new(Self {
                gets: Mutex::new(gets.into()),
                searches: Mutex::new(searches.into()),
                get_queries: Mutex::new(Vec::new()),
                search_queries: Mutex::new(Vec::new()),
            })
        }

        fn get_count(&self) -> usize {
            self.get_queries.lock().expect("get queries").len()
        }

        fn search_count(&self) -> usize {
            self.search_queries.lock().expect("search queries").len()
        }
    }

    struct BlockingClient {
        started: tokio::sync::Notify,
        release: tokio::sync::Notify,
        blocked_once: std::sync::atomic::AtomicBool,
    }

    #[async_trait]
    impl LyricsClient for BlockingClient {
        async fn get(&self, _query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
            if !self
                .blocked_once
                .swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                self.started.notify_one();
                self.release.notified().await;
            }
            Ok(Vec::new())
        }

        async fn search(&self, _query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
            Ok(Vec::new())
        }
    }

    #[async_trait]
    impl LyricsClient for FakeClient {
        async fn get(&self, query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
            self.get_queries
                .lock()
                .expect("get queries")
                .push(query.clone());
            self.gets
                .lock()
                .expect("get responses")
                .pop_front()
                .unwrap_or_else(|| Ok(Vec::new()))
        }

        async fn search(&self, query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
            self.search_queries
                .lock()
                .expect("search queries")
                .push(query.clone());
            self.searches
                .lock()
                .expect("search responses")
                .pop_front()
                .unwrap_or_else(|| Ok(Vec::new()))
        }
    }

    fn track_query(duration: f64) -> TrackQuery {
        TrackQuery::new("Song", "Artist", "Album", Some(duration))
    }

    #[tokio::test]
    async fn prefers_a_synced_exact_match() {
        let synced = track(1, "Song", "Artist", 180.0, Some("[00:01.00]Synced\n"), None);
        let client = FakeClient::new(vec![Ok(vec![]), Ok(vec![synced])], vec![]);
        let lookup = LyricsLookup::new(client.clone());

        let outcome = lookup
            .lookup_at(&track_query(180.0), 1000.0)
            .await
            .expect("lookup works");

        let LookupOutcome::Found(record) = outcome else {
            panic!("expected lyrics");
        };
        assert!(record.synced);
        assert_eq!(record.text, "[00:01.00]Synced\n");
        assert_eq!(record.key, record_key("Song", "Artist"));
        assert_eq!(
            record.original.as_ref().and_then(|item| item.lrclib_id),
            Some(1)
        );
        assert_eq!(client.search_count(), 0);
    }

    #[tokio::test]
    async fn searches_when_the_exact_match_is_only_plain_text() {
        let plain = track(1, "Song", "Artist", 180.0, None, Some("Plain\n"));
        let synced = track(2, "Song", "Artist", 181.0, Some("[00:01.00]Better\n"), None);
        let client = FakeClient::new(
            vec![Ok(Vec::new()), Ok(vec![plain])],
            vec![Ok(vec![synced])],
        );
        let lookup = LyricsLookup::new(client);

        let outcome = lookup
            .lookup_at(&track_query(180.0), 1000.0)
            .await
            .expect("lookup works");

        let LookupOutcome::Found(record) = outcome else {
            panic!("expected lyrics");
        };
        assert!(record.synced);
        assert_eq!(record.text, "[00:01.00]Better\n");
    }

    #[tokio::test]
    async fn keeps_the_exact_plain_match_when_no_synced_match_is_near() {
        let plain = track(1, "Song", "Artist", 180.0, None, Some("Plain\n"));
        let far = track(2, "Other", "Artist", 300.0, Some("[00:01.00]Far\n"), None);
        let client = FakeClient::new(vec![Ok(Vec::new()), Ok(vec![plain])], vec![Ok(vec![far])]);
        let lookup = LyricsLookup::new(client);

        let outcome = lookup
            .lookup_at(&track_query(180.0), 1000.0)
            .await
            .expect("lookup works");

        let LookupOutcome::Found(record) = outcome else {
            panic!("expected lyrics");
        };
        assert!(!record.synced);
        assert_eq!(record.text, "Plain\n");
    }

    #[tokio::test]
    async fn rejects_search_results_that_are_too_far_from_the_duration() {
        let far = track(1, "Song", "Artist", 300.0, Some("[00:01.00]Far\n"), None);
        let client = FakeClient::new(vec![Ok(Vec::new()), Ok(Vec::new())], vec![Ok(vec![far])]);
        let lookup = LyricsLookup::new(client);

        let outcome = lookup
            .lookup_at(&track_query(180.0), 1000.0)
            .await
            .expect("lookup works");

        assert_eq!(outcome, LookupOutcome::Missing { fetched_at: 1000.0 });
    }

    #[tokio::test]
    async fn caches_misses_for_one_hour() {
        let client = FakeClient::new(vec![Ok(Vec::new()), Ok(Vec::new())], vec![Ok(vec![])]);
        let lookup = LyricsLookup::new(client.clone());

        assert_eq!(
            lookup
                .lookup_at(&track_query(180.0), 1000.0)
                .await
                .expect("first lookup"),
            LookupOutcome::Missing { fetched_at: 1000.0 }
        );
        let calls_after_first = client.get_count() + client.search_count();

        assert_eq!(
            lookup
                .lookup_at(&track_query(180.0), 2000.0)
                .await
                .expect("cached lookup"),
            LookupOutcome::Missing { fetched_at: 1000.0 }
        );
        assert_eq!(
            client.get_count() + client.search_count(),
            calls_after_first
        );

        lookup
            .lookup_at(&track_query(180.0), 5000.0)
            .await
            .expect("expired lookup");
        assert!(client.get_count() + client.search_count() > calls_after_first);
    }

    #[tokio::test]
    async fn returns_pending_for_a_duplicate_concurrent_lookup() {
        let client = Arc::new(BlockingClient {
            started: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            blocked_once: std::sync::atomic::AtomicBool::new(false),
        });
        let lookup = Arc::new(LyricsLookup::new(client.clone()));

        let first = tokio::spawn({
            let lookup = lookup.clone();
            async move { lookup.lookup_at(&track_query(180.0), 1000.0).await }
        });

        client.started.notified().await;
        let second = lookup
            .lookup_at(&track_query(180.0), 1001.0)
            .await
            .expect("second lookup");
        assert_eq!(second, LookupOutcome::Pending);

        client.release.notify_one();
        let first = first.await.expect("task joins").expect("first lookup");
        assert_eq!(first, LookupOutcome::Missing { fetched_at: 1000.0 });
    }

    #[tokio::test]
    async fn does_not_cache_a_total_network_failure() {
        let client = FakeClient::new(
            vec![
                Err(LyricsError::Request("down".to_owned())),
                Err(LyricsError::Request("down".to_owned())),
            ],
            vec![Err(LyricsError::Request("down".to_owned()))],
        );
        let lookup = LyricsLookup::new(client.clone());

        let error = lookup
            .lookup_at(&track_query(180.0), 1000.0)
            .await
            .expect_err("network failure is reported");
        assert!(matches!(error, LyricsError::Request(_)));

        let client = FakeClient::new(vec![Ok(Vec::new()), Ok(Vec::new())], vec![Ok(vec![])]);
        let lookup = LyricsLookup::new(client.clone());
        lookup
            .lookup_at(&track_query(180.0), 1000.0)
            .await
            .expect("retry succeeds");
        assert!(client.get_count() > 0);
    }

    #[tokio::test]
    async fn empty_titles_are_missing_without_a_request() {
        let client = FakeClient::new(vec![], vec![]);
        let lookup = LyricsLookup::new(client.clone());

        let outcome = lookup
            .lookup_at(&TrackQuery::new("", "Artist", "", None), 1000.0)
            .await
            .expect("lookup works");

        assert_eq!(outcome, LookupOutcome::Missing { fetched_at: 1000.0 });
        assert_eq!(client.get_count(), 0);
        assert_eq!(client.search_count(), 0);
    }

    #[tokio::test]
    async fn metadata_save_uses_the_track_metadata_and_cleans_the_query() {
        let synced = track(
            7,
            "Clean Song",
            "Real Artist",
            180.0,
            Some("[00:01.00]Line\n"),
            None,
        );
        let client = FakeClient::new(vec![Ok(Vec::new()), Ok(vec![synced])], vec![]);
        let lookup = LyricsLookup::new(client.clone());

        let outcome = lookup
            .lookup_at(
                &TrackQuery::new(
                    "Real Artist - Clean Song (Official Video)",
                    "Real Artist - Topic",
                    "Album",
                    Some(180.4),
                ),
                1000.0,
            )
            .await
            .expect("lookup works");

        let LookupOutcome::Found(record) = outcome else {
            panic!("expected lyrics");
        };
        assert_eq!(record.title, "Real Artist - Clean Song (Official Video)");
        assert_eq!(record.artist, "Real Artist - Topic");
        assert_eq!(record.duration, Some(180.4));
        let queries = client.get_queries.lock().expect("get queries");
        assert_eq!(queries[0].title, "Clean Song");
        assert_eq!(queries[1].duration, Some(180));
    }

    #[test]
    fn parses_get_and_search_payloads() {
        let get_body = r#"{
            "id": 12,
            "trackName": "Song",
            "artistName": "Artist",
            "albumName": "Album",
            "duration": 180.0,
            "plainLyrics": "Plain",
            "syncedLyrics": "[00:01.00]Synced"
        }"#;
        let tracks = parse_get_json(reqwest::StatusCode::OK, get_body).expect("get parses");
        assert_eq!(tracks.len(), 1);
        assert!(tracks[0].has_synced());

        let search_body = r#"[{"id": 1, "trackName": "Song"}]"#;
        let tracks =
            parse_search_json(reqwest::StatusCode::OK, search_body).expect("search parses");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].track_name, "Song");
        assert!(!tracks[0].usable());
    }

    #[test]
    fn treats_404_as_an_empty_result_and_other_errors_as_failures() {
        assert!(
            parse_get_json(reqwest::StatusCode::NOT_FOUND, "")
                .expect("404 is empty")
                .is_empty()
        );
        assert!(
            parse_search_json(reqwest::StatusCode::NOT_FOUND, "")
                .expect("404 is empty")
                .is_empty()
        );
        assert!(matches!(
            parse_get_json(reqwest::StatusCode::INTERNAL_SERVER_ERROR, ""),
            Err(LyricsError::Request(_))
        ));
        assert!(matches!(
            parse_search_json(reqwest::StatusCode::OK, "not json"),
            Err(LyricsError::Decode(_))
        ));
    }

    #[test]
    fn builds_cleaned_queries() {
        let query = LookupQuery::new(
            "Artist - Song (Official Video)",
            "Artist - Topic",
            " Album ",
            Some(180.4),
        )
        .expect("query builds");
        assert_eq!(query.title, "Song");
        assert_eq!(query.artist, "Artist");
        assert_eq!(query.album, "Album");
        assert_eq!(query.duration, Some(180));
        assert_eq!(query.without_album().album, "");
        assert!(LookupQuery::new("", "Artist", "", None).is_none());
    }
}
