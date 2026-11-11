use std::time::Instant;

use nico_karaoke_core::{
    LibrarySummary, LyricLine, LyricRecord, LyricsSource, PlayerInfo, PlayerSnapshot,
};
use serde::Serialize;

use crate::watcher::{LyricsView, WatcherState};

#[derive(Debug, Serialize)]
pub struct TrackDto {
    pub player: String,
    pub label: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_id: String,
    pub length: Option<f64>,
    pub position: Option<f64>,
    pub age: f64,
    pub playing: bool,
    pub status: String,
    pub can_seek: bool,
    pub can_play: bool,
    pub rate: f64,
}

impl TrackDto {
    pub fn from_snapshot(snapshot: &PlayerSnapshot, now: Instant) -> Self {
        Self {
            player: snapshot.player.clone(),
            label: snapshot.label.clone(),
            title: snapshot.title.clone(),
            artist: snapshot.artist.clone(),
            album: snapshot.album.clone(),
            track_id: snapshot.track_id.clone(),
            length: snapshot.length,
            position: snapshot.position,
            age: now
                .saturating_duration_since(snapshot.sampled_at)
                .as_secs_f64(),
            playing: snapshot.playing,
            status: snapshot.status.clone(),
            can_seek: snapshot.can_seek,
            can_play: snapshot.can_play,
            rate: snapshot.rate,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OriginalDto {
    pub source: LyricsSource,
    pub fetched_at: f64,
    pub lrclib_id: Option<i64>,
    pub synced: bool,
    pub same: bool,
}

#[derive(Debug, Serialize)]
pub struct LyricsDto {
    pub state: &'static str,
    pub signature: Option<String>,
    pub record_key: Option<String>,
    pub synced: bool,
    pub offset: f64,
    pub source: Option<LyricsSource>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<f64>,
    pub text: String,
    pub lines: Vec<LyricLine>,
    pub lrclib_id: Option<i64>,
    pub fetched_at: f64,
    pub updated_at: f64,
    pub original: Option<OriginalDto>,
    pub error: Option<&'static str>,
}

impl LyricsDto {
    pub fn none(signature: Option<String>) -> Self {
        Self::empty("none", signature, None)
    }

    pub fn loading(record_key: String, signature: Option<String>) -> Self {
        let mut dto = Self::empty("loading", signature, Some(record_key));
        dto.state = "loading";
        dto
    }

    pub fn missing(fetched_at: f64, record_key: String, signature: Option<String>) -> Self {
        let mut dto = Self::empty("missing", signature, Some(record_key));
        dto.fetched_at = fetched_at;
        dto
    }

    pub fn error(code: &'static str, signature: Option<String>) -> Self {
        let mut dto = Self::empty("error", signature, None);
        dto.error = Some(code);
        dto
    }

    pub fn found(record: LyricRecord, signature: Option<String>) -> Self {
        let original = record.original.as_ref().map(|original| OriginalDto {
            source: original.source,
            fetched_at: original.fetched_at,
            lrclib_id: original.lrclib_id,
            synced: original.synced,
            same: original.text == record.text,
        });

        Self {
            state: "found",
            signature,
            record_key: Some(record.key),
            synced: record.synced,
            offset: record.offset,
            source: Some(record.source),
            title: record.title,
            artist: record.artist,
            album: record.album,
            duration: record.duration,
            text: record.text,
            lines: record.lines,
            lrclib_id: record.lrclib_id,
            fetched_at: record.fetched_at,
            updated_at: record.updated_at,
            original,
            error: None,
        }
    }

    fn empty(state: &'static str, signature: Option<String>, record_key: Option<String>) -> Self {
        Self {
            state,
            signature,
            record_key,
            synced: false,
            offset: 0.0,
            source: None,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            duration: None,
            text: String::new(),
            lines: Vec::new(),
            lrclib_id: None,
            fetched_at: 0.0,
            updated_at: 0.0,
            original: None,
            error: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NowDto {
    pub track: Option<TrackDto>,
    pub players: Vec<PlayerInfo>,
    pub lyrics: LyricsDto,
}

impl NowDto {
    pub fn from_state(state: &WatcherState, now: Instant) -> Self {
        let signature = state.media.snapshot.as_ref().map(PlayerSnapshot::signature);
        Self {
            track: state
                .media
                .snapshot
                .as_ref()
                .map(|snapshot| TrackDto::from_snapshot(snapshot, now)),
            players: state.media.players.clone(),
            lyrics: lyrics_from_view(&state.lyrics, signature),
        }
    }
}

pub fn lyrics_from_view(view: &LyricsView, signature: Option<String>) -> LyricsDto {
    match view {
        LyricsView::None => LyricsDto::none(signature),
        LyricsView::Loading { key } => LyricsDto::loading(key.clone(), signature),
        LyricsView::Found(record) => LyricsDto::found((**record).clone(), signature),
        LyricsView::Missing { key, fetched_at } => {
            LyricsDto::missing(*fetched_at, key.clone(), signature)
        }
        LyricsView::Error { code } => LyricsDto::error(code, signature),
    }
}

#[derive(Debug, Serialize)]
pub struct LibraryItemDto {
    #[serde(flatten)]
    pub summary: LibrarySummary,
    pub current: bool,
}

#[derive(Debug, Serialize)]
pub struct LibraryDto {
    pub items: Vec<LibraryItemDto>,
    pub current: Option<String>,
    pub signature: Option<String>,
}

impl LibraryDto {
    pub fn new(
        items: Vec<LibrarySummary>,
        current: Option<String>,
        signature: Option<String>,
    ) -> Self {
        let items = items
            .into_iter()
            .map(|summary| LibraryItemDto {
                current: current.as_deref() == Some(summary.key.as_str()),
                summary,
            })
            .collect();
        Self {
            items,
            current,
            signature,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LibraryItemResponse {
    pub lyrics: LyricsDto,
    pub current: bool,
}

#[derive(Debug, Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct ErrorDto {
    pub error: &'static str,
}

impl ErrorDto {
    pub fn new(error: &'static str) -> Self {
        Self { error }
    }
}
