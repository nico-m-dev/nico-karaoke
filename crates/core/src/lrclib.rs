use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::library::{LIBRARY_VERSION, LyricRecord, LyricsSource, OriginalLyrics, normalize_text};
use crate::lyrics::parse_lrc;
use crate::title::{credit_alternatives, normalize, record_key, split_title};

pub const MISS_TTL_SECONDS: f64 = 3600.0;
pub const DURATION_TOLERANCE_SECONDS: f64 = 5.0;
const MISS_CACHE_LIMIT: usize = 2048;

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

    pub fn query_pairs(&self) -> Vec<(&'static str, String)> {
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

        let Some(_guard) = self.begin_lookup(&key) else {
            return Ok(LookupOutcome::Pending);
        };

        let mut first_error = None;
        match self.find_item(&base, track).await {
            Ok(Some(item)) => {
                return Ok(LookupOutcome::Found(Box::new(record_from(
                    &item, track, now,
                ))));
            }
            Ok(None) => {}
            Err(error) => first_error = Some(error),
        }

        // A YouTube title can carry the performer while the channel name is unrelated, so a
        // direct query for the raw metadata finds nothing. Only in that case, retry with the
        // credit inside the title and store the result under that cleaner metadata.
        for (artist, title) in credit_alternatives(&track.title, &track.artist) {
            let Some(base) = LookupQuery::new(&title, &artist, &track.album, track.duration) else {
                continue;
            };
            let mut variant = track.clone();
            variant.title = title;
            variant.artist = artist;
            match self.find_item(&base, &variant).await {
                Ok(Some(item)) => {
                    return Ok(LookupOutcome::Found(Box::new(record_from(
                        &item, &variant, now,
                    ))));
                }
                Ok(None) => {}
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }

        // A partial or total network failure must not become an hour-long miss.
        if let Some(error) = first_error {
            return Err(error);
        }

        self.remember_miss(key, now);
        Ok(LookupOutcome::Missing { fetched_at: now })
    }

    async fn find_item(
        &self,
        base: &LookupQuery,
        track: &TrackQuery,
    ) -> Result<Option<LrclibTrack>, LyricsError> {
        let without_album = base.without_album();
        let duration = track
            .duration
            .filter(|value| value.is_finite() && *value > 0.0);

        let mut exact = Vec::new();
        let mut searched = Vec::new();
        let mut seen = HashSet::new();
        let mut errors = Vec::new();

        if !base.album.is_empty() {
            match self.client.get(base).await {
                Ok(items) => add_candidates(&mut exact, items, &mut seen),
                Err(error) => errors.push(error),
            }
        }

        // An exact album match with synced lyrics is already the best possible result.
        if !exact.iter().any(LrclibTrack::has_synced) {
            match self.client.get(&without_album).await {
                Ok(items) => add_candidates(&mut exact, items, &mut seen),
                Err(error) => errors.push(error),
            }
        }

        if !exact.iter().any(LrclibTrack::has_synced) {
            match self.client.search(&without_album).await {
                Ok(items) => {
                    // Search results are fuzzy. Keep only results whose title and artist are
                    // related to the requested track; otherwise a cover or a different song
                    // with a coincidentally similar duration can win.
                    let related: Vec<_> = items
                        .into_iter()
                        .filter(|item| related_to(item, track))
                        .collect();
                    add_candidates(&mut searched, related, &mut seen);
                }
                Err(error) => errors.push(error),
            }
        }

        if let Some(item) = select(&exact, &searched, duration) {
            return Ok(Some(item.clone()));
        }

        // A partial or total network failure must not become an hour-long miss.
        if let Some(error) = errors.into_iter().next() {
            return Err(error);
        }

        Ok(None)
    }

    fn begin_lookup<'a>(&'a self, key: &str) -> Option<InFlightGuard<'a>> {
        let inserted = self
            .in_flight
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key.to_owned());
        inserted.then(|| InFlightGuard {
            lookup: self,
            key: key.to_owned(),
        })
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
            Some(fetched_at) if now < fetched_at => None,
            Some(fetched_at) if now - fetched_at <= self.miss_ttl_seconds => Some(fetched_at),
            Some(_) => {
                misses.remove(key);
                None
            }
            None => None,
        }
    }

    fn remember_miss(&self, key: String, now: f64) {
        let mut misses = self
            .misses
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // A long session can otherwise remember every track it has ever seen. Sweep expired
        // entries, then drop the oldest half if the cache is still at its limit.
        if misses.len() >= MISS_CACHE_LIMIT {
            misses.retain(|_, fetched_at| {
                now >= *fetched_at && now - *fetched_at <= self.miss_ttl_seconds
            });
            if misses.len() >= MISS_CACHE_LIMIT {
                let mut entries: Vec<(String, f64)> = misses
                    .iter()
                    .map(|(key, fetched_at)| (key.clone(), *fetched_at))
                    .collect();
                entries.sort_by(|left, right| left.1.total_cmp(&right.1));
                let remove_count = misses.len() / 2;
                for (key, _) in entries.into_iter().take(remove_count) {
                    misses.remove(&key);
                }
            }
        }
        misses.insert(key, now);
    }
}

/// Removes the in-flight marker even when the lookup future is cancelled or dropped.
struct InFlightGuard<'a> {
    lookup: &'a LyricsLookup,
    key: String,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.lookup.end_lookup(&self.key);
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

/// Whether a fuzzy search result is plausibly the requested song. This is deliberately more
/// generous than an exact key match, but it rejects unrelated titles and artists.
fn related_to(item: &LrclibTrack, track: &TrackQuery) -> bool {
    let (query_title, query_artist) = split_title(&track.title, &track.artist);
    let (item_title, item_artist) = split_title(&item.track_name, &item.artist_name);
    let query_title = normalize(&query_title);
    let item_title = normalize(&item_title);
    if query_title.is_empty() || item_title.is_empty() {
        return false;
    }

    let (shorter, longer) = if item_title.chars().count() <= query_title.chars().count() {
        (&item_title, &query_title)
    } else {
        (&query_title, &item_title)
    };
    let title_matches = item_title == query_title
        || (shorter.chars().count() >= 3 && longer.contains(shorter.as_str()));
    if !title_matches {
        return false;
    }

    let query_artist = normalize(&query_artist);
    if query_artist.is_empty() {
        return true;
    }
    let item_artist = normalize(&item_artist);
    item_artist.is_empty()
        || item_artist == query_artist
        || item_artist.contains(&query_artist)
        || query_artist.contains(&item_artist)
}

fn select<'a>(
    exact: &'a [LrclibTrack],
    searched: &'a [LrclibTrack],
    duration: Option<f64>,
) -> Option<&'a LrclibTrack> {
    // A fallback query can match a live or extended cut exactly; prefer the one whose length
    // also fits the track that is playing.
    if let Some(item) = exact
        .iter()
        .find(|item| item.has_synced() && near(item, duration))
    {
        return Some(item);
    }

    if let Some(item) = exact.iter().find(|item| item.has_synced()) {
        return Some(item);
    }

    // LRCLIB already returns search results in relevance order. Keep that order and use the
    // duration only to reject results, instead of re-ranking by duration.
    if let Some(item) = searched
        .iter()
        .find(|item| item.has_synced() && near(item, duration))
    {
        return Some(item);
    }

    if let Some(item) = exact.first() {
        return Some(item);
    }

    searched.iter().find(|item| near(item, duration))
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
        metadata: document.metadata,
        extras: document.extras,
        lrclib_id: item.id,
        fetched_at: now,
        updated_at: now,
        original,
    }
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
        calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl LyricsClient for BlockingClient {
        async fn get(&self, _query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
    async fn prefers_an_exact_match_that_also_fits_the_duration() {
        let far = track(1, "Song", "Artist", 300.0, Some("[00:01.00]Far\n"), None);
        let near = track(2, "Song", "Artist", 181.0, Some("[00:01.00]Near\n"), None);
        let client = FakeClient::new(vec![Ok(vec![far, near])], vec![]);
        let lookup = LyricsLookup::new(client);

        let outcome = lookup
            .lookup_at(&track_query(180.0), 1000.0)
            .await
            .expect("lookup works");

        let LookupOutcome::Found(record) = outcome else {
            panic!("expected lyrics");
        };
        assert_eq!(record.text, "[00:01.00]Near\n");
    }

    #[tokio::test]
    async fn short_circuits_after_a_synced_album_match() {
        let synced = track(1, "Song", "Artist", 180.0, Some("[00:01.00]Synced\n"), None);
        let client = FakeClient::new(vec![Ok(vec![synced])], vec![]);
        let lookup = LyricsLookup::new(client.clone());

        let outcome = lookup
            .lookup_at(&track_query(180.0), 1000.0)
            .await
            .expect("lookup works");

        assert!(matches!(outcome, LookupOutcome::Found(_)));
        assert_eq!(client.get_count(), 1);
        assert_eq!(client.search_count(), 0);
    }

    #[tokio::test]
    async fn partial_failure_is_not_cached() {
        let client = FakeClient::new(
            vec![
                Err(LyricsError::Request("album down".to_owned())),
                Ok(Vec::new()),
            ],
            vec![Ok(Vec::new())],
        );
        let lookup = LyricsLookup::new(client.clone());

        let error = lookup
            .lookup_at(&track_query(180.0), 1000.0)
            .await
            .expect_err("partial failure is reported");
        assert!(matches!(error, LyricsError::Request(_)));

        let calls_before = client.get_count() + client.search_count();
        let outcome = lookup
            .lookup_at(&track_query(180.0), 1001.0)
            .await
            .expect("retry reaches the client");
        assert_eq!(outcome, LookupOutcome::Missing { fetched_at: 1001.0 });
        assert!(client.get_count() + client.search_count() > calls_before);
    }

    #[tokio::test]
    async fn invalid_duration_is_treated_as_unknown() {
        let synced = track(1, "Song", "Artist", 200.0, Some("[00:01.00]Synced\n"), None);
        let client = FakeClient::new(vec![Ok(Vec::new()), Ok(Vec::new())], vec![Ok(vec![synced])]);
        let lookup = LyricsLookup::new(client);

        let outcome = lookup
            .lookup_at(
                &TrackQuery::new("Song", "Artist", "Album", Some(f64::NAN)),
                1000.0,
            )
            .await
            .expect("lookup works");

        assert!(matches!(outcome, LookupOutcome::Found(_)));
    }

    #[tokio::test]
    async fn backwards_clock_does_not_use_a_future_miss() {
        let client = FakeClient::new(vec![Ok(Vec::new()), Ok(Vec::new())], vec![Ok(vec![])]);
        let lookup = LyricsLookup::new(client.clone());

        assert_eq!(
            lookup
                .lookup_at(&track_query(180.0), 4600.5)
                .await
                .expect("first lookup"),
            LookupOutcome::Missing { fetched_at: 4600.5 }
        );
        let calls_before = client.get_count() + client.search_count();

        assert_eq!(
            lookup
                .lookup_at(&track_query(180.0), 500.0)
                .await
                .expect("backwards lookup"),
            LookupOutcome::Missing { fetched_at: 500.0 }
        );
        assert!(client.get_count() + client.search_count() > calls_before);
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
    async fn ignores_search_results_with_a_different_title() {
        let wrong = track(
            1,
            "Totally Different",
            "Artist",
            180.0,
            Some("[00:01.00]Wrong\n"),
            None,
        );
        let client = FakeClient::new(vec![Ok(Vec::new()), Ok(Vec::new())], vec![Ok(vec![wrong])]);
        let lookup = LyricsLookup::new(client);

        let outcome = lookup
            .lookup_at(&track_query(180.0), 1000.0)
            .await
            .expect("lookup works");

        assert_eq!(outcome, LookupOutcome::Missing { fetched_at: 1000.0 });
    }

    #[tokio::test]
    async fn accepts_a_search_result_with_a_parenthetical_title() {
        let item = track(
            2,
            "Hey (Live)",
            "Artist",
            181.0,
            Some("[00:01.00]Live\n"),
            None,
        );
        let client = FakeClient::new(vec![Ok(Vec::new()), Ok(Vec::new())], vec![Ok(vec![item])]);
        let lookup = LyricsLookup::new(client);

        let outcome = lookup
            .lookup_at(
                &TrackQuery::new("Hey", "Artist", "Album", Some(180.0)),
                1000.0,
            )
            .await
            .expect("lookup works");

        assert!(matches!(outcome, LookupOutcome::Found(_)));
    }

    #[test]
    fn caps_the_miss_cache() {
        let lookup = LyricsLookup::new(FakeClient::new(Vec::new(), Vec::new()));
        for index in 0..(MISS_CACHE_LIMIT + 100) {
            lookup.remember_miss(format!("key-{index}"), 1000.0);
        }

        let size = lookup.misses.read().expect("miss cache").len();
        assert!(size <= MISS_CACHE_LIMIT);
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
            calls: std::sync::atomic::AtomicUsize::new(0),
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
    async fn cancelled_lookup_releases_the_in_flight_guard() {
        let client = Arc::new(BlockingClient {
            started: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            blocked_once: std::sync::atomic::AtomicBool::new(false),
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let lookup = Arc::new(LyricsLookup::new(client.clone()));

        let task = tokio::spawn({
            let lookup = lookup.clone();
            async move { lookup.lookup_at(&track_query(180.0), 1000.0).await }
        });
        client.started.notified().await;
        task.abort();
        let _ = task.await;

        let outcome = lookup
            .lookup_at(&track_query(180.0), 2000.0)
            .await
            .expect("retry works");
        assert_eq!(outcome, LookupOutcome::Missing { fetched_at: 2000.0 });
        assert!(
            client.calls.load(std::sync::atomic::Ordering::SeqCst) >= 2,
            "the retry must reach the client"
        );
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
    async fn retries_with_a_decorated_credit_when_the_channel_is_unrelated() {
        let found = track(
            9,
            "Feraline",
            "Combichrist",
            312.0,
            Some("[00:01.00]Line\n"),
            None,
        );
        let client = FakeClient::new(vec![Ok(vec![]), Ok(vec![found])], vec![Ok(vec![])]);
        let lookup = LyricsLookup::new(client.clone());
        let video = TrackQuery::new(
            "Combichrist - Feraline (Official Lyric Video)",
            "Out Of Line Music",
            "",
            Some(313.0),
        );

        let outcome = lookup
            .lookup_at(&video, 1000.0)
            .await
            .expect("lookup works");
        let LookupOutcome::Found(record) = outcome else {
            panic!("expected the credit fallback to find lyrics");
        };
        assert_eq!(record.title, "Feraline");
        assert_eq!(record.artist, "Combichrist");
        assert_eq!(record.key, record_key("Feraline", "Combichrist"));

        let queries = client.get_queries.lock().expect("get queries");
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[1].title, "Feraline");
        assert_eq!(queries[1].artist, "Combichrist");
    }

    #[tokio::test]
    async fn retries_with_a_plain_credit_when_the_channel_is_unrelated() {
        let found = track(
            10,
            "Party Time",
            "The Northern Boys",
            182.0,
            Some("[00:01.00]Line\n"),
            None,
        );
        let client = FakeClient::new(vec![Ok(vec![]), Ok(vec![found])], vec![Ok(vec![])]);
        let lookup = LyricsLookup::new(client.clone());
        let video = TrackQuery::new(
            "The Northern Boys - Party Time",
            "Sindhu World",
            "",
            Some(182.0),
        );

        let outcome = lookup
            .lookup_at(&video, 1000.0)
            .await
            .expect("lookup works");
        let LookupOutcome::Found(record) = outcome else {
            panic!("expected the plain credit fallback to find lyrics");
        };
        assert_eq!(record.title, "Party Time");
        assert_eq!(record.artist, "The Northern Boys");
    }

    #[tokio::test]
    async fn does_not_retry_with_a_version_word_as_the_title() {
        let client = FakeClient::new(vec![Ok(vec![])], vec![Ok(vec![])]);
        let lookup = LyricsLookup::new(client.clone());
        let track = TrackQuery::new("Song - Live", "Someone Else", "", Some(200.0));

        let outcome = lookup
            .lookup_at(&track, 1000.0)
            .await
            .expect("lookup works");

        assert_eq!(outcome, LookupOutcome::Missing { fetched_at: 1000.0 });
        assert_eq!(client.get_count(), 1);
        assert_eq!(client.search_count(), 1);
    }

    #[tokio::test]
    async fn a_failed_fallback_is_reported_and_not_cached_as_a_miss() {
        let client = FakeClient::new(
            vec![
                Err(LyricsError::Request("down".to_owned())),
                Err(LyricsError::Request("down".to_owned())),
            ],
            vec![Err(LyricsError::Request("down".to_owned()))],
        );
        let lookup = LyricsLookup::new(client.clone());
        let video = TrackQuery::new(
            "Combichrist - Feraline (Official Lyric Video)",
            "Out Of Line Music",
            "",
            Some(313.0),
        );

        let error = lookup
            .lookup_at(&video, 1000.0)
            .await
            .expect_err("a network failure is reported");
        assert!(matches!(error, LyricsError::Request(_)));

        let before = client.get_count() + client.search_count();
        let _ = lookup.lookup_at(&video, 2000.0).await;
        assert!(
            client.get_count() + client.search_count() > before,
            "a failure must not be cached as a miss"
        );
    }

    #[tokio::test]
    async fn a_fallback_can_recover_from_a_failed_strict_lookup() {
        let found = track(
            9,
            "Feraline",
            "Combichrist",
            312.0,
            Some("[00:01.00]Line\n"),
            None,
        );
        let client = FakeClient::new(
            vec![
                Err(LyricsError::Request("down".to_owned())),
                Ok(vec![found]),
            ],
            vec![Err(LyricsError::Request("down".to_owned()))],
        );
        let lookup = LyricsLookup::new(client);
        let video = TrackQuery::new(
            "Combichrist - Feraline (Official Lyric Video)",
            "Out Of Line Music",
            "",
            Some(313.0),
        );

        let outcome = lookup
            .lookup_at(&video, 1000.0)
            .await
            .expect("the fallback succeeds");

        let LookupOutcome::Found(record) = outcome else {
            panic!("expected the fallback to recover");
        };
        assert_eq!(record.title, "Feraline");
        assert_eq!(record.artist, "Combichrist");
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
