use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde::{Deserialize, Serialize};

use crate::lyrics::{LyricLine, parse_lrc};
use crate::title::{normalize, record_key, slugify, split_title};

pub const LIBRARY_VERSION: u32 = 1;
pub const LIBRARY_DIR_NAME: &str = "library";
pub const LOG_FILE_NAME: &str = "nico-karaoke.log";
const MAX_OFFSET_SECONDS: f64 = 10.0;
const MATCH_DURATION_TOLERANCE_SECONDS: f64 = 10.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LyricsSource {
    Lrclib,
    #[default]
    Manual,
    Tap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OriginalLyrics {
    pub text: String,
    #[serde(default)]
    pub source: LyricsSource,
    #[serde(default)]
    pub fetched_at: f64,
    #[serde(default)]
    pub lrclib_id: Option<i64>,
    #[serde(default)]
    pub synced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricRecord {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub source: LyricsSource,
    #[serde(default)]
    pub synced: bool,
    #[serde(default)]
    pub offset: f64,
    #[serde(default)]
    pub text: String,
    #[serde(skip, default)]
    pub lines: Vec<LyricLine>,
    #[serde(default)]
    pub lrclib_id: Option<i64>,
    #[serde(default)]
    pub fetched_at: f64,
    #[serde(default)]
    pub updated_at: f64,
    #[serde(default)]
    pub original: Option<OriginalLyrics>,
}

impl LyricRecord {
    pub fn new(
        key: impl Into<String>,
        title: impl Into<String>,
        artist: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            version: LIBRARY_VERSION,
            key: key.into(),
            title: title.into(),
            artist: artist.into(),
            album: String::new(),
            duration: None,
            source: LyricsSource::Manual,
            synced: false,
            offset: 0.0,
            text: text.into(),
            lines: Vec::new(),
            lrclib_id: None,
            fetched_at: 0.0,
            updated_at: 0.0,
            original: None,
        }
    }
}

impl Default for LyricRecord {
    fn default() -> Self {
        Self::new("", "", "", "")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibrarySummary {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<f64>,
    pub source: LyricsSource,
    pub synced: bool,
    pub offset: f64,
    pub updated_at: f64,
    pub line_count: usize,
    pub has_ends: bool,
    pub has_original: bool,
}

impl From<&LyricRecord> for LibrarySummary {
    fn from(record: &LyricRecord) -> Self {
        Self {
            key: record.key.clone(),
            title: record.title.clone(),
            artist: record.artist.clone(),
            album: record.album.clone(),
            duration: record.duration,
            source: record.source,
            synced: record.synced,
            offset: record.offset,
            updated_at: record.updated_at,
            line_count: record
                .lines
                .iter()
                .filter(|line| !line.text.trim().is_empty())
                .count(),
            has_ends: record.lines.iter().any(|line| line.end.is_some()),
            has_original: record
                .original
                .as_ref()
                .is_some_and(|original| !original.text.trim().is_empty()),
        }
    }
}

#[derive(Default)]
struct State {
    records: HashMap<String, LyricRecord>,
    files: HashMap<String, PathBuf>,
}

pub struct Library {
    dir: PathBuf,
    state: RwLock<State>,
    warnings: Vec<String>,
}

impl Library {
    /// Load the library from disk, creating the directory when needed. Invalid records are
    /// skipped and reported through `warnings()` instead of taking the whole library down.
    pub fn open(directory: impl Into<PathBuf>) -> Result<Self, LibraryError> {
        let dir = directory.into();
        fs::create_dir_all(&dir)?;

        let mut state = State::default();
        let mut warnings = Vec::new();
        let mut paths = Vec::new();

        for entry in fs::read_dir(&dir)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warnings.push(format!("cannot read a library entry: {error}"));
                    continue;
                }
            };
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("json") {
                paths.push(path);
            }
        }
        paths.sort();

        for path in paths {
            match load_record(&path) {
                Ok(record) => {
                    if state.records.contains_key(&record.key) {
                        warnings.push(format!(
                            "duplicate key {} in {}",
                            record.key,
                            relative_display(&path, &dir)
                        ));
                    }
                    state.files.insert(record.key.clone(), path);
                    state.records.insert(record.key.clone(), record);
                }
                Err(error) => {
                    warnings.push(format!("{}: {error}", relative_display(&path, &dir)));
                }
            }
        }

        Ok(Self {
            dir,
            state: RwLock::new(state),
            warnings,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn len(&self) -> usize {
        self.read_state().records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, key: &str) -> Option<LyricRecord> {
        self.read_state().records.get(key).cloned()
    }

    /// Find the record that belongs to a track snapshot. YouTube metadata can omit the
    /// artist or add a channel/video decoration that the exact key cannot see, so fall back
    /// to a normalised title match. Duration breaks ties between covers of the same title.
    pub fn find_key(&self, title: &str, artist: &str, duration: Option<f64>) -> Option<String> {
        let exact = record_key(title, artist);
        let state = self.read_state();
        if state.records.contains_key(&exact) {
            return Some(exact);
        }

        let (target_title, target_artist) = split_title(title, artist);
        let title_key = normalize(&target_title);
        let artist_key = normalize(&target_artist);
        if title_key.is_empty() {
            return None;
        }

        let mut candidates: Vec<&LyricRecord> = state
            .records
            .values()
            .filter(|record| {
                let (record_title, record_artist) = split_title(&record.title, &record.artist);
                normalize(&record_title) == title_key
                    && (artist_key.is_empty() || normalize(&record_artist) == artist_key)
            })
            .collect();

        if candidates.len() <= 1 {
            return candidates.first().map(|record| record.key.clone());
        }

        let duration = duration.filter(|value| value.is_finite() && *value > 0.0)?;
        candidates.retain(|record| {
            record
                .duration
                .is_some_and(|value| (value - duration).abs() <= MATCH_DURATION_TOLERANCE_SECONDS)
        });
        if candidates.len() == 1 {
            candidates.first().map(|record| record.key.clone())
        } else {
            None
        }
    }

    pub fn summaries(&self) -> Vec<LibrarySummary> {
        let mut items: Vec<_> = self
            .read_state()
            .records
            .values()
            .map(LibrarySummary::from)
            .collect();
        items.sort_by(|left, right| {
            right
                .updated_at
                .total_cmp(&left.updated_at)
                .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
        });
        items
    }

    pub fn search(&self, query: &str) -> Vec<LibrarySummary> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return self.summaries();
        }

        self.summaries()
            .into_iter()
            .filter(|item| {
                item.title.to_lowercase().contains(&query)
                    || item.artist.to_lowercase().contains(&query)
                    || item.album.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Save a record. The readable `.lrc` file is the source of truth: if it was edited by
    /// hand, a metadata-only update keeps that edit instead of overwriting it.
    pub fn put(&self, mut record: LyricRecord) -> Result<LyricRecord, LibraryError> {
        let key = record.key.trim().to_owned();
        validate_key(&key)?;

        let mut state = self.write_state();
        let current = state.records.get(&key).cloned();
        let old_path = state.files.get(&key).cloned();

        let external = old_path
            .as_ref()
            .and_then(|path| fs::read_to_string(path.with_extension("lrc")).ok())
            .map(|text| normalize_text(&text))
            .unwrap_or_default();
        let current_text = current
            .as_ref()
            .map(|record| record.text.as_str())
            .unwrap_or_default();
        let caller_text = normalize_text(&record.text);
        let mut manual_edit_wins = false;

        // If a hand-edited .lrc differs from the stored text, a metadata-only update keeps
        // the manual edit. A caller that deliberately writes new text wins instead.
        if !external.is_empty() && external != current_text && caller_text == current_text {
            record.text = external;
            record.source = LyricsSource::Manual;
            manual_edit_wins = true;
        }

        if let Some(current) = &current {
            if caller_text == current.text {
                if !manual_edit_wins && record.source == LyricsSource::Manual {
                    record.source = current.source;
                }
                if record.lrclib_id.is_none() {
                    record.lrclib_id = current.lrclib_id;
                }
                if record.fetched_at <= 0.0 {
                    record.fetched_at = current.fetched_at;
                }
            }
            // The first fetched text stays available even when the working text changes.
            if record.original.is_none() {
                record.original = current.original.clone();
            }
        }

        if record.updated_at <= 0.0 {
            record.updated_at = unix_now();
        }

        let shaped = shape(record)?;
        let path = self.path_for(&shaped);
        fs::create_dir_all(&self.dir)?;

        write_atomic(&path.with_extension("lrc"), &shaped.text)?;
        let payload = serde_json::to_string_pretty(&shaped)? + "\n";
        write_atomic(&path, &payload)?;

        if let Some(old_path) = old_path {
            if old_path != path {
                remove_pair(&old_path)?;
            }
        }

        state.records.insert(key.clone(), shaped.clone());
        state.files.insert(key, path);
        Ok(shaped)
    }

    pub fn delete(&self, key: &str) -> Result<bool, LibraryError> {
        let mut state = self.write_state();
        let path = state.files.get(key).cloned();
        if let Some(path) = &path {
            remove_pair(path)?;
        }
        state.files.remove(key);
        state.records.remove(key);
        Ok(path.is_some())
    }

    pub fn path_for(&self, record: &LyricRecord) -> PathBuf {
        let parts = [slugify(&record.artist), slugify(&record.title)];
        let stem = parts
            .iter()
            .filter(|part| !part.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("-");
        let stem = if stem.is_empty() {
            "song".to_owned()
        } else {
            stem
        };
        let suffix: String = record
            .key
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
            })
            .take(10)
            .collect();
        let suffix = if suffix.is_empty() {
            "record".to_owned()
        } else {
            suffix
        };
        self.dir.join(format!("{stem}-{suffix}.json"))
    }

    fn read_state(&self) -> RwLockReadGuard<'_, State> {
        self.state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_state(&self) -> RwLockWriteGuard<'_, State> {
        self.state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn default_version() -> u32 {
    LIBRARY_VERSION
}

fn relative_display(path: &Path, directory: &Path) -> String {
    path.strip_prefix(directory)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn load_record(path: &Path) -> Result<LyricRecord, LibraryError> {
    let raw = fs::read_to_string(path)?;
    let mut record: LyricRecord = serde_json::from_str(&raw)?;
    if record.version > LIBRARY_VERSION {
        return Err(LibraryError::UnsupportedVersion(record.version));
    }

    let external = fs::read_to_string(path.with_extension("lrc")).ok();
    if let Some(external) = external {
        let external = normalize_text(&external);
        if !external.is_empty() && external != normalize_text(&record.text) {
            record.text = external;
            record.source = LyricsSource::Manual;
        }
    }

    shape(record)
}

fn shape(mut record: LyricRecord) -> Result<LyricRecord, LibraryError> {
    let key = record.key.trim().to_owned();
    validate_key(&key)?;

    let text = normalize_text(&record.text);
    if text.trim().is_empty() {
        return Err(LibraryError::EmptyText);
    }

    let document = parse_lrc(&text);
    record.version = LIBRARY_VERSION;
    record.key = key;
    record.title = record.title.trim().to_owned();
    record.artist = record.artist.trim().to_owned();
    record.album = record.album.trim().to_owned();
    record.duration = record
        .duration
        .filter(|duration| duration.is_finite() && *duration > 0.0);
    record.synced = document.synced;
    record.lines = document.lines;
    record.text = text;
    record.offset = if record.offset.is_finite() {
        record.offset.clamp(-MAX_OFFSET_SECONDS, MAX_OFFSET_SECONDS)
    } else {
        0.0
    };
    record.original = clean_original(record.original);
    Ok(record)
}

fn clean_original(original: Option<OriginalLyrics>) -> Option<OriginalLyrics> {
    let mut original = original?;
    original.text = normalize_text(&original.text);
    if original.text.trim().is_empty() {
        return None;
    }
    original.synced = parse_lrc(&original.text).synced;
    Some(original)
}

pub fn normalize_text(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim_start_matches('\u{feff}').trim_end();
    if text.is_empty() {
        String::new()
    } else {
        format!("{text}\n")
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), LibraryError> {
    let parent = path.parent().ok_or_else(|| {
        LibraryError::Io(std::io::Error::new(
            ErrorKind::InvalidInput,
            "path has no parent directory",
        ))
    })?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(text.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| LibraryError::Io(error.error))?;
    Ok(())
}

fn remove_pair(path: &Path) -> Result<(), LibraryError> {
    for target in [path.with_extension("lrc"), path.to_path_buf()] {
        match fs::remove_file(&target) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(source) => return Err(LibraryError::Remove { source }),
        }
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<(), LibraryError> {
    if key.is_empty() {
        return Err(LibraryError::MissingKey);
    }
    let valid = key.len() <= 64
        && key.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        });
    if valid {
        Ok(())
    } else {
        Err(LibraryError::InvalidKey)
    }
}

fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("library I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid library JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("record has no key")]
    MissingKey,
    #[error("invalid library key")]
    InvalidKey,
    #[error("record has no text")]
    EmptyText,
    #[error("unsupported library version {0}")]
    UnsupportedVersion(u32),
    #[error("could not remove a library file: {source}")]
    Remove { source: std::io::Error },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(key: &str, title: &str, artist: &str, text: &str) -> LyricRecord {
        let mut record = LyricRecord::new(key, title, artist, text);
        record.source = LyricsSource::Lrclib;
        record.updated_at = 1.0;
        record
    }

    fn files_for(library: &Library, record: &LyricRecord) -> (PathBuf, PathBuf) {
        let json = library.path_for(record);
        (json.clone(), json.with_extension("lrc"))
    }

    #[test]
    fn opens_an_empty_library_and_creates_the_directory() {
        let temp = tempfile::tempdir().expect("temp directory");
        let directory = temp.path().join("library");
        let library = Library::open(&directory).expect("library opens");

        assert!(directory.is_dir());
        assert!(library.is_empty());
        assert!(library.warnings().is_empty());
    }

    #[test]
    fn saves_loads_and_summarises_records() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");

        let mut item = record(
            "key-1",
            "Song",
            "Artist",
            "[00:01.00]First\n[00:02.50]Second\n",
        );
        item.album = "Album".to_owned();
        item.duration = Some(180.0);
        item.original = Some(OriginalLyrics {
            text: "[00:01.00]Original\n".to_owned(),
            source: LyricsSource::Lrclib,
            fetched_at: 2.0,
            lrclib_id: Some(42),
            synced: false,
        });

        let saved = library.put(item.clone()).expect("record saves");
        assert!(saved.synced);
        assert_eq!(saved.lines.len(), 2);

        let (json_path, lrc_path) = files_for(&library, &saved);
        assert!(json_path.is_file());
        assert!(lrc_path.is_file());
        assert_eq!(
            fs::read_to_string(&lrc_path).expect("lrc readable"),
            saved.text
        );
        let json = fs::read_to_string(&json_path).expect("json readable");
        assert!(json.contains("\"version\": 1"));
        assert!(!json.contains("\"lines\""));

        drop(library);
        let reopened = Library::open(temp.path()).expect("library reopens");
        let loaded = reopened.get("key-1").expect("record loads");
        assert_eq!(loaded, saved);
        assert_eq!(
            loaded.original.as_ref().map(|item| item.lrclib_id),
            Some(Some(42))
        );

        let summaries = reopened.summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].line_count, 2);
        assert!(summaries[0].has_original);
        assert!(!summaries[0].has_ends);
    }

    #[test]
    fn finds_a_record_when_the_current_metadata_is_thinner() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let key = record_key("I Don't Want You In Here", "Solar Fake");
        let mut item = record(
            &key,
            "I Don't Want You In Here",
            "Solar Fake",
            "[00:01.00]Line\n",
        );
        item.duration = Some(213.0);
        library.put(item).expect("record saves");

        assert_eq!(
            library.find_key("I Don't Want You In Here", "Solar Fake", Some(213.0)),
            Some(key.clone())
        );
        assert_eq!(
            library.find_key("I Don't Want You In Here", "", Some(213.0)),
            Some(key.clone())
        );
        assert_eq!(
            library.find_key(
                "Solar Fake - I Don't Want You In Here (Official Video Clip)",
                "",
                Some(213.0)
            ),
            Some(key)
        );
    }

    #[test]
    fn does_not_guess_between_same_title_covers_without_a_duration() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let first = record_key("Same Title", "Artist A");
        let second = record_key("Same Title", "Artist B");
        for (key, artist, duration) in [(&first, "Artist A", 200.0), (&second, "Artist B", 300.0)] {
            let mut item = record(key, "Same Title", artist, "Line\n");
            item.duration = Some(duration);
            library.put(item).expect("record saves");
        }

        assert_eq!(library.find_key("Same Title", "", None), None);
        assert_eq!(
            library.find_key("Same Title", "", Some(201.0)),
            Some(first.clone())
        );
        assert_eq!(
            library.find_key("Same Title", "Artist B", None),
            Some(second)
        );
    }

    #[test]
    fn manual_lrc_edit_wins_over_a_metadata_only_update() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let item = record("key-1", "Song", "Artist", "First\nSecond\n");
        let saved = library.put(item.clone()).expect("record saves");
        let (_, lrc_path) = files_for(&library, &saved);

        fs::write(&lrc_path, "First\nEdited by hand\n").expect("manual edit");

        let mut metadata_update = saved.clone();
        metadata_update.offset = 0.5;
        let updated = library.put(metadata_update).expect("metadata update saves");

        assert_eq!(updated.text, "First\nEdited by hand\n");
        assert_eq!(updated.source, LyricsSource::Manual);
        assert_eq!(updated.offset, 0.5);
    }

    #[test]
    fn reopening_marks_a_hand_edited_file_as_manual() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let saved = library
            .put(record("key-1", "Song", "Artist", "First\n"))
            .expect("record saves");
        let (_, lrc_path) = files_for(&library, &saved);
        fs::write(&lrc_path, "Edited by hand\n").expect("manual edit");
        drop(library);

        let reopened = Library::open(temp.path()).expect("library reopens");
        let loaded = reopened.get("key-1").expect("record loads");
        assert_eq!(loaded.text, "Edited by hand\n");
        assert_eq!(loaded.source, LyricsSource::Manual);
    }

    #[test]
    fn rejects_unsafe_keys_and_keeps_paths_inside_the_library() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let mut item = record("../../escape", "Song", "Artist", "Line\n");

        let error = library
            .put(item.clone())
            .expect_err("unsafe key is rejected");
        assert!(matches!(error, LibraryError::InvalidKey));

        item.key = "key/with/slash".to_owned();
        let error = library.put(item).expect_err("slash key is rejected");
        assert!(matches!(error, LibraryError::InvalidKey));

        let path = library.path_for(&LyricRecord::new(
            "../../escape",
            "Song",
            "Artist",
            "Line\n",
        ));
        assert!(path.starts_with(temp.path()));
    }

    #[test]
    fn preserves_provenance_on_a_fresh_save() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let mut item = record("key-1", "Song", "Artist", "First\n");
        item.lrclib_id = Some(42);
        item.fetched_at = 111.0;
        item.original = Some(OriginalLyrics {
            text: "[00:01.00]Original\n".to_owned(),
            source: LyricsSource::Lrclib,
            fetched_at: 111.0,
            lrclib_id: Some(42),
            synced: true,
        });
        library.put(item).expect("record saves");

        let fresh = LyricRecord::new("key-1", "Song", "Artist", "First\n");
        let saved = library.put(fresh).expect("fresh save succeeds");

        assert_eq!(saved.source, LyricsSource::Lrclib);
        assert_eq!(saved.lrclib_id, Some(42));
        assert_eq!(saved.fetched_at, 111.0);
        assert!(saved.original.is_some());
    }

    #[test]
    fn preserves_provenance_when_a_manual_edit_wins() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let mut item = record("key-1", "Song", "Artist", "First\n");
        item.lrclib_id = Some(42);
        item.fetched_at = 111.0;
        item.original = Some(OriginalLyrics {
            text: "[00:01.00]Original\n".to_owned(),
            source: LyricsSource::Lrclib,
            fetched_at: 111.0,
            lrclib_id: Some(42),
            synced: true,
        });
        let saved = library.put(item).expect("record saves");
        let (_, lrc_path) = files_for(&library, &saved);
        fs::write(&lrc_path, "Hand edit\n").expect("manual edit");

        let fresh = LyricRecord::new("key-1", "Song", "Artist", "First\n");
        let updated = library.put(fresh).expect("fresh metadata save succeeds");

        assert_eq!(updated.text, "Hand edit\n");
        assert_eq!(updated.source, LyricsSource::Manual);
        assert_eq!(updated.lrclib_id, Some(42));
        assert_eq!(updated.fetched_at, 111.0);
        assert!(updated.original.is_some());
    }

    #[test]
    fn stamps_updated_at_when_missing() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let mut item = record("key-1", "Song", "Artist", "Line\n");
        item.updated_at = 0.0;
        let saved = library.put(item).expect("record saves");

        assert!(saved.updated_at > 0.0);
        assert_eq!(library.summaries()[0].updated_at, saved.updated_at);
    }

    #[test]
    fn explicit_text_save_wins_over_a_manual_edit() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let saved = library
            .put(record("key-1", "Song", "Artist", "First\n"))
            .expect("record saves");
        let (_, lrc_path) = files_for(&library, &saved);
        fs::write(&lrc_path, "Hand edit\n").expect("manual edit");

        let mut explicit = saved.clone();
        explicit.text = "New text\n".to_owned();
        let updated = library.put(explicit).expect("explicit save succeeds");

        assert_eq!(updated.text, "New text\n");
    }

    #[test]
    fn deleting_removes_both_files() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let saved = library
            .put(record("key-1", "Song", "Artist", "First\n"))
            .expect("record saves");
        let (json_path, lrc_path) = files_for(&library, &saved);

        assert!(library.delete("key-1").expect("delete succeeds"));
        assert!(!json_path.exists());
        assert!(!lrc_path.exists());
        assert!(library.get("key-1").is_none());
        assert!(!library.delete("key-1").expect("second delete succeeds"));
    }

    #[test]
    fn changing_the_title_moves_the_files() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let first = library
            .put(record("key-1", "Old Title", "Artist", "First\n"))
            .expect("record saves");
        let (old_json, old_lrc) = files_for(&library, &first);

        let mut renamed = first.clone();
        renamed.title = "New Title".to_owned();
        let second = library.put(renamed).expect("rename saves");
        let (new_json, new_lrc) = files_for(&library, &second);

        assert!(!old_json.exists());
        assert!(!old_lrc.exists());
        assert!(new_json.exists());
        assert!(new_lrc.exists());
    }

    #[test]
    fn searches_title_artist_and_album() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let mut first = record("key-1", "Alpha", "One", "First\n");
        first.album = "Blue".to_owned();
        let mut second = record("key-2", "Beta", "Two", "Second\n");
        second.album = "Red".to_owned();
        library.put(first).expect("first saves");
        library.put(second).expect("second saves");

        assert_eq!(library.search("alpha").len(), 1);
        assert_eq!(library.search("two").len(), 1);
        assert_eq!(library.search("red").len(), 1);
        assert_eq!(library.search("").len(), 2);
        assert!(library.search("missing").is_empty());
    }

    #[test]
    fn skips_bad_json_and_unsupported_versions_with_warnings() {
        let temp = tempfile::tempdir().expect("temp directory");
        fs::write(temp.path().join("bad.json"), "{not json").expect("bad file");
        fs::write(
            temp.path().join("future.json"),
            "{\"version\":99,\"key\":\"future\",\"text\":\"Line\\n\"}",
        )
        .expect("future file");

        let library = Library::open(temp.path()).expect("library opens");
        assert!(library.is_empty());
        assert_eq!(library.warnings().len(), 2);
    }

    #[test]
    fn plain_lyrics_stay_unsynchronised() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let saved = library
            .put(record("key-1", "Song", "Artist", "Just words\n"))
            .expect("record saves");

        assert!(!saved.synced);
        assert_eq!(saved.lines.len(), 1);
        assert_eq!(saved.lines[0].t, None);
    }

    #[test]
    fn clamps_offset_and_ignores_invalid_duration() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let mut item = record("key-1", "Song", "Artist", "Line\n");
        item.offset = 100.0;
        item.duration = Some(-4.0);
        let saved = library.put(item).expect("record saves");

        assert_eq!(saved.offset, 10.0);
        assert_eq!(saved.duration, None);
    }

    #[test]
    fn normalises_original_text_and_recomputes_synced() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        let mut item = record("key-1", "Song", "Artist", "Line\n");
        item.original = Some(OriginalLyrics {
            text: "[00:01.00]Original\r\n".to_owned(),
            source: LyricsSource::Lrclib,
            fetched_at: 0.0,
            lrclib_id: None,
            synced: false,
        });
        let saved = library.put(item).expect("record saves");
        let original = saved.original.expect("original kept");

        assert_eq!(original.text, "[00:01.00]Original\n");
        assert!(original.synced);
    }

    #[test]
    fn leaves_no_temporary_files_behind() {
        let temp = tempfile::tempdir().expect("temp directory");
        let library = Library::open(temp.path()).expect("library opens");
        library
            .put(record("key-1", "Song", "Artist", "Line\n"))
            .expect("record saves");

        let names: Vec<_> = fs::read_dir(temp.path())
            .expect("directory readable")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|name| !name.contains(".tmp")));
    }
}
