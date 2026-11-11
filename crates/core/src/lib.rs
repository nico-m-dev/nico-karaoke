//! Domain logic for Nico Karaoke.
//!
//! This crate must stay independent of HTTP, desktop-shell, and platform APIs. Media
//! providers and lyrics clients are injected through traits so tests can run without a real
//! player or network.

pub mod config;
pub mod library;
pub mod lrclib;
pub mod lyrics;
pub mod media;
pub mod title;

pub use config::{APP_NAME, APP_VERSION, AppConfig, ConfigError, DEFAULT_PORT, LaunchMode, USAGE};
pub use library::{
    LIBRARY_DIR_NAME, LIBRARY_VERSION, LOG_FILE_NAME, Library, LibraryError, LibrarySummary,
    LyricRecord, LyricsSource, OriginalLyrics, normalize_text,
};
pub use lrclib::{
    DURATION_TOLERANCE_SECONDS, LookupOutcome, LookupQuery, LrclibTrack, LyricsClient, LyricsError,
    LyricsLookup, MISS_TTL_SECONDS, TrackQuery,
};
pub use lyrics::{
    LrcDocument, LyricLine, format_timestamp, is_music_marker, parse_lrc, parse_timestamp, to_lrc,
};
pub use media::{
    FakeMediaProvider, MediaError, MediaProvider, MediaState, MediaTracker, MediaUpdate,
    PlaybackAction, PlaybackControl, PlayerInfo, PlayerSnapshot,
};
pub use title::{clean, normalize, record_key, slugify, split_title, strip_channel};
