use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use nico_karaoke_core::{
    Library, LibraryError, LibrarySummary, LookupOutcome, LyricRecord, LyricsError, LyricsLookup,
    LyricsSource, MISS_TTL_SECONDS, MediaError, MediaProvider, MediaState, MediaTracker,
    OriginalLyrics, PlaybackControl, TrackQuery, record_key,
};
use thiserror::Error;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;

const POLL_INTERVAL: Duration = Duration::from_millis(350);
const FAILURE_COOLDOWN: Duration = Duration::from_secs(30);
const MEDIA_LOG_COOLDOWN: Duration = Duration::from_secs(30);
const PENDING_RETRIES: usize = 20;
const PENDING_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq)]
pub enum LyricsView {
    None,
    Loading { key: String },
    Found(Box<LyricRecord>),
    Missing { key: String, fetched_at: f64 },
    Error { code: &'static str },
}

impl LyricsView {
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::Loading { key } => Some(key),
            Self::Found(record) => Some(&record.key),
            Self::Missing { key, .. } => Some(key),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WatcherState {
    pub media: MediaState,
    pub lyrics: LyricsView,
}

#[derive(Debug, Error)]
pub enum WatcherError {
    #[error("no current track")]
    NoCurrentTrack,
    #[error("the track changed")]
    TrackChanged,
    #[error("lyrics not found")]
    LyricsNotFound,
    #[error("lyrics lookup is still running")]
    LyricsPending,
    #[error("no original kept")]
    NoOriginal,
    #[error("invalid value")]
    InvalidValue,
    #[error("library error: {0}")]
    Library(#[from] LibraryError),
    #[error("library task failed: {0}")]
    Task(String),
    #[error("lyrics error: {0}")]
    Lyrics(#[from] LyricsError),
}

struct Cooldown {
    generation: u64,
    until: Instant,
}

struct MatchCache {
    revision: u64,
    title: String,
    artist: String,
    duration: Option<u64>,
    key: String,
}

pub struct NewLibraryItem {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub text: String,
    pub source: LyricsSource,
    pub duration: Option<f64>,
    pub original_text: Option<String>,
}

pub struct Watcher {
    provider: Arc<dyn MediaProvider>,
    lookup: Arc<LyricsLookup>,
    library: Arc<Library>,
    media: AsyncMutex<MediaTracker>,
    state: RwLock<WatcherState>,
    in_flight: Mutex<Option<(u64, u64)>>,
    cooldown: Mutex<Option<Cooldown>>,
    ensure_lock: Mutex<()>,
    match_cache: Mutex<Option<MatchCache>>,
    media_error_at: Mutex<Option<Instant>>,
    /// Deleted keys with the revision at which they disappeared. An in-flight lookup that
    /// started before that revision must not resurrect the record.
    deleted: Mutex<HashMap<String, u64>>,
    revision: AtomicU64,
}

impl Watcher {
    pub fn new(
        provider: Arc<dyn MediaProvider>,
        lookup: Arc<LyricsLookup>,
        library: Arc<Library>,
    ) -> Self {
        Self {
            provider,
            lookup,
            library,
            media: AsyncMutex::new(MediaTracker::new()),
            state: RwLock::new(WatcherState {
                media: empty_media_state(),
                lyrics: LyricsView::None,
            }),
            in_flight: Mutex::new(None),
            cooldown: Mutex::new(None),
            ensure_lock: Mutex::new(()),
            match_cache: Mutex::new(None),
            media_error_at: Mutex::new(None),
            deleted: Mutex::new(HashMap::new()),
            revision: AtomicU64::new(0),
        }
    }

    pub fn start(self: &Arc<Self>) {
        let watcher = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(POLL_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                watcher.tick().await;
            }
        });
    }

    pub async fn tick(self: &Arc<Self>) {
        // Sample outside the tracker lock. A slow or stuck player must not block play/pause or
        // player selection, which need the same lock.
        let snapshots = match self.provider.snapshots().await {
            Ok(snapshots) => snapshots,
            Err(error) => {
                self.note_media_error(error);
                return;
            }
        };
        *self
            .media_error_at
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;

        // Apply the sample and write the visible state in one critical section, so a concurrent
        // choose_player() cannot be rolled back by an older sample.
        {
            let mut media = self.media.lock().await;
            let update = media.update(snapshots);
            self.state
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .media = update.state;
        }

        self.ensure_lyrics();
    }

    fn note_media_error(&self, error: MediaError) {
        let now = Instant::now();
        let should_log = {
            let mut last = self
                .media_error_at
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match *last {
                Some(at) if now.duration_since(at) < MEDIA_LOG_COOLDOWN => false,
                _ => {
                    *last = Some(now);
                    true
                }
            }
        };
        if should_log {
            tracing::warn!(error = %error, "media poll failed");
        } else {
            tracing::debug!(error = %error, "media poll still failing");
        }
    }

    pub fn state(&self) -> WatcherState {
        self.state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn library_items(&self) -> (Vec<LibrarySummary>, Option<String>) {
        let state = self.state();
        let current =
            state.media.snapshot.as_ref().map(|snapshot| {
                self.matched_key(&snapshot.title, &snapshot.artist, snapshot.length)
            });
        (self.library.summaries(), current)
    }

    pub fn library_item(&self, key: &str) -> Option<LyricRecord> {
        self.library.get(key)
    }

    pub fn is_current_key(&self, key: &str) -> bool {
        self.state()
            .media
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                self.matched_key(&snapshot.title, &snapshot.artist, snapshot.length) == key
            })
    }

    /// Key for the followed track, using the library's title fallback when a provider omits
    /// the artist or adds video decorations.
    fn matched_key(&self, title: &str, artist: &str, duration: Option<f64>) -> String {
        let revision = self.revision.load(Ordering::SeqCst);
        let duration = duration
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| (value * 1000.0).round() as u64);

        {
            let cache = self
                .match_cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(cache) = cache.as_ref() {
                if cache.revision == revision
                    && cache.title == title
                    && cache.artist == artist
                    && cache.duration == duration
                {
                    return cache.key.clone();
                }
            }
        }

        let key = self
            .library
            .find_key(title, artist, duration.map(|value| value as f64 / 1000.0))
            .unwrap_or_else(|| record_key(title, artist));
        *self
            .match_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(MatchCache {
            revision,
            title: title.to_owned(),
            artist: artist.to_owned(),
            duration,
            key: key.clone(),
        });
        key
    }

    pub async fn choose_player(self: &Arc<Self>, player: &str) -> bool {
        {
            let mut media = self.media.lock().await;
            if !media.choose(player) {
                return false;
            }
            let state = media.state();
            self.state
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .media = state;
        }

        self.bump_revision();
        self.ensure_lyrics();
        true
    }

    pub async fn control(&self, control: PlaybackControl) -> Result<(), MediaError> {
        let media = self.media.lock().await;
        media.control(self.provider.as_ref(), control).await
    }

    pub async fn save_current_text(
        self: &Arc<Self>,
        text: String,
        source: LyricsSource,
        original_text: Option<String>,
        expected_track_id: Option<&str>,
        expected_signature: Option<&str>,
    ) -> Result<LyricRecord, WatcherError> {
        if text.trim().is_empty() {
            return Err(WatcherError::InvalidValue);
        }

        let state = self.state();
        let snapshot = state
            .media
            .snapshot
            .clone()
            .ok_or(WatcherError::NoCurrentTrack)?;
        if expected_track_id
            .is_some_and(|expected| !expected.is_empty() && expected != snapshot.track_id)
        {
            return Err(WatcherError::TrackChanged);
        }
        if expected_signature
            .is_some_and(|expected| !expected.is_empty() && expected != snapshot.signature())
        {
            return Err(WatcherError::TrackChanged);
        }

        let generation = state.media.generation;
        let key = self.matched_key(&snapshot.title, &snapshot.artist, snapshot.length);
        let existing = self.library.get(&key);
        let mut record = existing.clone().unwrap_or_else(|| {
            LyricRecord::new(
                key.clone(),
                snapshot.title.clone(),
                snapshot.artist.clone(),
                text.clone(),
            )
        });

        if existing.is_none() {
            record.title = snapshot.title.clone();
            record.artist = snapshot.artist.clone();
        }
        if !snapshot.album.is_empty() {
            record.album = snapshot.album.clone();
        }
        if snapshot.length.is_some() {
            record.duration = snapshot.length;
        }
        record.text = text.clone();
        record.source = source;
        record.updated_at = 0.0;
        set_original(&mut record, source, &text, original_text);

        self.forget_deleted(&key);
        let saved = save_record(self.library.clone(), record).await?;
        self.bump_revision();

        if !self.is_generation(generation) {
            return Err(WatcherError::TrackChanged);
        }
        self.set_lyrics(generation, LyricsView::Found(Box::new(saved.clone())));
        Ok(saved)
    }

    pub async fn save_library_text(
        self: &Arc<Self>,
        key: &str,
        text: String,
        source: LyricsSource,
        original_text: Option<String>,
    ) -> Result<LyricRecord, WatcherError> {
        if text.trim().is_empty() {
            return Err(WatcherError::InvalidValue);
        }

        let existing = self.library.get(key).ok_or(WatcherError::LyricsNotFound)?;
        let mut record = existing;
        record.text = text.clone();
        record.source = source;
        record.updated_at = 0.0;
        set_original(&mut record, source, &text, original_text);

        self.forget_deleted(key);
        let saved = save_record(self.library.clone(), record).await?;
        self.bump_revision();
        if self.is_current_key(key) {
            let generation = self.state().media.generation;
            self.set_lyrics(generation, LyricsView::Found(Box::new(saved.clone())));
        }
        Ok(saved)
    }

    pub async fn create_library_item(
        self: &Arc<Self>,
        item: NewLibraryItem,
    ) -> Result<LyricRecord, WatcherError> {
        let NewLibraryItem {
            title,
            artist,
            album,
            text,
            source,
            duration,
            original_text,
        } = item;
        if title.trim().is_empty() || text.trim().is_empty() {
            return Err(WatcherError::InvalidValue);
        }

        let key = record_key(&title, &artist);
        let existing = self.library.get(&key);
        let mut record = existing.clone().unwrap_or_else(|| {
            LyricRecord::new(key.clone(), title.clone(), artist.clone(), text.clone())
        });
        record.title = title;
        record.artist = artist;
        if !album.is_empty() {
            record.album = album;
        }
        if duration.is_some() {
            record.duration = duration;
        }
        record.text = text.clone();
        record.source = source;
        record.updated_at = 0.0;
        set_original(&mut record, source, &text, original_text);

        self.forget_deleted(&key);
        let saved = save_record(self.library.clone(), record).await?;
        self.bump_revision();
        if self.is_current_key(&key) {
            let generation = self.state().media.generation;
            self.set_lyrics(generation, LyricsView::Found(Box::new(saved.clone())));
        }
        Ok(saved)
    }

    pub async fn delete_library_item(self: &Arc<Self>, key: &str) -> Result<bool, WatcherError> {
        let library = self.library.clone();
        let key_owned = key.to_owned();
        let removed = tokio::task::spawn_blocking(move || library.delete(&key_owned))
            .await
            .map_err(|error| WatcherError::Task(error.to_string()))??;

        if removed {
            self.bump_revision();
            self.deleted
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(key.to_owned(), self.revision.load(Ordering::SeqCst));
            if self.is_current_key(key) {
                let generation = self.state().media.generation;
                self.set_lyrics(
                    generation,
                    LyricsView::Missing {
                        key: key.to_owned(),
                        fetched_at: unix_now(),
                    },
                );
            }
        }
        Ok(removed)
    }

    pub async fn restore_original(
        self: &Arc<Self>,
        key: &str,
    ) -> Result<LyricRecord, WatcherError> {
        let record = self.library.get(key).ok_or(WatcherError::LyricsNotFound)?;
        let original = record.original.ok_or(WatcherError::NoOriginal)?;
        let source = original.source;
        self.save_library_text(key, original.text, source, None)
            .await
    }

    pub async fn set_offset(
        self: &Arc<Self>,
        value: f64,
        expected_signature: Option<&str>,
    ) -> Result<LyricRecord, WatcherError> {
        if !value.is_finite() {
            return Err(WatcherError::InvalidValue);
        }
        let state = self.state();
        let snapshot = state
            .media
            .snapshot
            .clone()
            .ok_or(WatcherError::NoCurrentTrack)?;
        if expected_signature
            .is_some_and(|expected| !expected.is_empty() && expected != snapshot.signature())
        {
            return Err(WatcherError::TrackChanged);
        }

        let key = self.matched_key(&snapshot.title, &snapshot.artist, snapshot.length);
        let generation = state.media.generation;
        let mut record = self.library.get(&key).ok_or(WatcherError::LyricsNotFound)?;
        record.offset = (value.clamp(-10.0, 10.0) * 100.0).round() / 100.0;
        let saved = save_record(self.library.clone(), record).await?;
        self.bump_revision();
        if !self.is_generation(generation) || !self.is_current_key(&key) {
            return Err(WatcherError::TrackChanged);
        }
        self.set_lyrics(generation, LyricsView::Found(Box::new(saved.clone())));
        Ok(saved)
    }

    pub async fn fetch(
        &self,
        title: &str,
        artist: &str,
        album: &str,
        length: Option<f64>,
    ) -> Result<LyricRecord, WatcherError> {
        let track = TrackQuery::new(title, artist, album, length);
        let mut attempts = 0;
        loop {
            match self.lookup.lookup(&track).await? {
                LookupOutcome::Found(record) => return Ok(*record),
                LookupOutcome::Missing { .. } => return Err(WatcherError::LyricsNotFound),
                LookupOutcome::Pending if attempts < PENDING_RETRIES => {
                    attempts += 1;
                    sleep(PENDING_DELAY).await;
                }
                LookupOutcome::Pending => return Err(WatcherError::LyricsPending),
            }
        }
    }

    /// Apply fetched lyrics that the user explicitly asked for. Unlike a background lookup,
    /// this replaces a manual working text and keeps the fetched original for restore.
    pub async fn save_fetched(
        self: &Arc<Self>,
        mut record: LyricRecord,
    ) -> Result<LyricRecord, WatcherError> {
        // A fetch for a video can carry a different title and channel than the record the
        // library already keeps for the same song. Reuse that record instead of adding a
        // duplicate under a second key. The fetched record carries the query metadata, so an
        // item opened by key still resolves back to itself.
        if let Some(existing) = self
            .library
            .find_key(&record.title, &record.artist, record.duration)
            .and_then(|key| self.library.get(&key))
        {
            // Keep the first stored original; a later fetch must not replace it.
            if existing.original.is_some() {
                record.original = existing.original;
            }
            record.key = existing.key;
            record.title = existing.title;
            record.artist = existing.artist;
            record.offset = existing.offset;
            if record.album.trim().is_empty() {
                record.album = existing.album;
            }
            if record.duration.is_none() {
                record.duration = existing.duration;
            }
        }
        record.updated_at = 0.0;
        self.forget_deleted(&record.key);
        let saved = save_record(self.library.clone(), record).await?;
        self.bump_revision();
        if self.is_current_key(&saved.key) {
            let generation = self.state().media.generation;
            self.set_lyrics(generation, LyricsView::Found(Box::new(saved.clone())));
        }
        Ok(saved)
    }

    fn ensure_lyrics(self: &Arc<Self>) {
        let _guard = self
            .ensure_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (snapshot, generation) = {
            let state = self
                .state
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (state.media.snapshot.clone(), state.media.generation)
        };
        let Some(snapshot) = snapshot else {
            self.set_lyrics(generation, LyricsView::None);
            return;
        };

        let key = self.matched_key(&snapshot.title, &snapshot.artist, snapshot.length);
        let revision = self.revision.load(Ordering::SeqCst);
        let in_flight = *self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let cooldown = self
            .cooldown
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|cooldown| (cooldown.generation, cooldown.until));

        {
            let state = self
                .state
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match &state.lyrics {
                LyricsView::Found(record) if record.key == key => return,
                LyricsView::Loading { key: loading }
                    if *loading == key
                        && in_flight.is_some_and(|(in_flight_generation, _)| {
                            in_flight_generation == generation
                        }) =>
                {
                    return;
                }
                LyricsView::Missing {
                    key: missing,
                    fetched_at,
                } if *missing == key && miss_is_fresh(*fetched_at, unix_now()) => {
                    return;
                }
                LyricsView::Error { .. }
                    if cooldown.is_some_and(|(cooldown_generation, until)| {
                        cooldown_generation == generation && Instant::now() < until
                    }) =>
                {
                    return;
                }
                _ => {}
            }
        }

        if let Some(record) = self.library.get(&key) {
            self.bump_revision();
            self.set_lyrics(generation, LyricsView::Found(Box::new(record)));
            return;
        }

        self.set_lyrics(generation, LyricsView::Loading { key: key.clone() });
        *self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((generation, revision));

        let watcher = self.clone();
        let lookup = self.lookup.clone();
        let library = self.library.clone();
        tokio::spawn(async move {
            let track = TrackQuery::new(
                snapshot.title,
                snapshot.artist,
                snapshot.album,
                snapshot.length,
            );

            let mut attempts = 0;
            let result = loop {
                if !watcher.is_generation(generation) {
                    watcher.clear_in_flight(generation, revision);
                    return;
                }
                match lookup.lookup(&track).await {
                    Ok(LookupOutcome::Pending) if attempts < PENDING_RETRIES => {
                        attempts += 1;
                        sleep(PENDING_DELAY).await;
                    }
                    other => break other,
                }
            };

            if !watcher.is_generation(generation) {
                watcher.clear_in_flight(generation, revision);
                return;
            }

            match result {
                Ok(LookupOutcome::Found(record)) => {
                    let record = *record;
                    // A delete after this lookup started must not be undone by fetched lyrics.
                    if watcher.deleted_after(&key, revision) {
                        watcher.clear_in_flight(generation, revision);
                        return;
                    }
                    // A save while the lookup was running wins over the fetched text. This is
                    // scoped to this track's key, so saving an unrelated song no longer throws
                    // the lookup away.
                    if let Some(existing) = library.get(&key) {
                        watcher.set_lyrics(generation, LyricsView::Found(Box::new(existing)));
                    } else {
                        if watcher.deleted_after(&key, revision) {
                            watcher.clear_in_flight(generation, revision);
                            return;
                        }
                        let to_save = record.clone();
                        let saved = tokio::task::spawn_blocking(move || library.put(to_save)).await;
                        let final_record = match saved {
                            Ok(Ok(saved)) => saved,
                            Ok(Err(error)) => {
                                tracing::warn!(error = %error, "could not save fetched lyrics");
                                record
                            }
                            Err(error) => {
                                tracing::warn!(error = %error, "library save task failed");
                                record
                            }
                        };
                        watcher.set_lyrics(generation, LyricsView::Found(Box::new(final_record)));
                    }
                }
                Ok(LookupOutcome::Missing { fetched_at }) => {
                    watcher.set_lyrics(
                        generation,
                        LyricsView::Missing {
                            key: key.clone(),
                            fetched_at,
                        },
                    );
                }
                Ok(LookupOutcome::Pending) => {
                    watcher.set_cooldown(generation, Duration::from_secs(5));
                    watcher.set_lyrics(
                        generation,
                        LyricsView::Error {
                            code: "lyrics_pending",
                        },
                    );
                }
                Err(error) => {
                    tracing::warn!(error = %error, "lyrics lookup failed");
                    watcher.set_cooldown(generation, FAILURE_COOLDOWN);
                    watcher.set_lyrics(
                        generation,
                        LyricsView::Error {
                            code: "lyrics_unavailable",
                        },
                    );
                }
            }
            watcher.clear_in_flight(generation, revision);
        });
    }

    fn is_generation(&self, generation: u64) -> bool {
        self.state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .media
            .generation
            == generation
    }

    fn set_lyrics(&self, generation: u64, lyrics: LyricsView) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.media.generation == generation {
            state.lyrics = lyrics;
        }
    }

    fn set_cooldown(&self, generation: u64, duration: Duration) {
        *self
            .cooldown
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Cooldown {
            generation,
            until: Instant::now() + duration,
        });
    }

    fn clear_in_flight(&self, generation: u64, revision: u64) {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *in_flight == Some((generation, revision)) {
            *in_flight = None;
        }
    }

    fn bump_revision(&self) {
        self.revision.fetch_add(1, Ordering::SeqCst);
    }

    fn forget_deleted(&self, key: &str) {
        self.deleted
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(key);
    }

    fn deleted_after(&self, key: &str, revision: u64) -> bool {
        self.deleted
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(key)
            .is_some_and(|deleted| *deleted > revision)
    }
}

fn empty_media_state() -> MediaState {
    MediaState {
        players: Vec::new(),
        snapshot: None,
        chosen: None,
        generation: 0,
    }
}

fn set_original(
    record: &mut LyricRecord,
    source: LyricsSource,
    text: &str,
    original_text: Option<String>,
) {
    let lrclib_id = record.lrclib_id;
    let fetched_at = record.fetched_at;
    let mut original = merge_original(record.original.take(), source, text, original_text);
    if let Some(original) = &mut original {
        if original.lrclib_id.is_none() {
            original.lrclib_id = lrclib_id;
        }
        if original.fetched_at <= 0.0 {
            original.fetched_at = fetched_at;
        }
    }
    record.original = original;
}

fn merge_original(
    current: Option<OriginalLyrics>,
    source: LyricsSource,
    text: &str,
    original_text: Option<String>,
) -> Option<OriginalLyrics> {
    if current.is_some() {
        return current;
    }

    let text = original_text
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            if source == LyricsSource::Lrclib {
                text.to_owned()
            } else {
                String::new()
            }
        });
    if text.trim().is_empty() {
        return None;
    }

    Some(OriginalLyrics {
        text,
        source: LyricsSource::Lrclib,
        fetched_at: unix_now(),
        lrclib_id: None,
        synced: false,
    })
}

async fn save_record(
    library: Arc<Library>,
    record: LyricRecord,
) -> Result<LyricRecord, WatcherError> {
    tokio::task::spawn_blocking(move || library.put(record))
        .await
        .map_err(|error| WatcherError::Task(error.to_string()))?
        .map_err(WatcherError::Library)
}

fn miss_is_fresh(fetched_at: f64, now: f64) -> bool {
    now >= fetched_at && now - fetched_at <= MISS_TTL_SECONDS
}

fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}
