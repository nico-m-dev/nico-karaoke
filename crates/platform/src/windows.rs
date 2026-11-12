use std::future::IntoFuture;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use nico_karaoke_core::{MediaError, MediaProvider, PlaybackAction, PlayerSnapshot};
use windows::Foundation::{DateTime, TimeSpan};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};

const SMTC_TIMEOUT: Duration = Duration::from_secs(4);
const WINDOWS_EPOCH_OFFSET_TICKS: i64 = 116_444_736_000_000_000;
const TICKS_PER_SECOND: f64 = 10_000_000.0;

const KNOWN_APPS: &[(&str, &str)] = &[
    ("spotify", "Spotify"),
    ("msedge", "Edge"),
    ("chrome", "Chrome"),
    ("brave", "Brave"),
    ("firefox", "Firefox"),
    ("vivaldi", "Vivaldi"),
    ("opera", "Opera"),
    ("tidal", "Tidal"),
    ("deezer", "Deezer"),
    ("vlc", "VLC"),
    ("foobar", "foobar2000"),
    ("music.ui", "Media Player"),
    ("zunemusic", "Media Player"),
    ("wmplayer", "Windows Media Player"),
];

pub struct SmtcMediaProvider {
    manager: Mutex<Option<GlobalSystemMediaTransportControlsSessionManager>>,
}

impl SmtcMediaProvider {
    pub fn new() -> Self {
        ensure_winrt();
        Self {
            manager: Mutex::new(None),
        }
    }

    async fn manager(
        &self,
    ) -> Result<GlobalSystemMediaTransportControlsSessionManager, MediaError> {
        if let Some(manager) = self
            .manager
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
        {
            return Ok(manager);
        }

        ensure_winrt();
        let manager = wait(
            GlobalSystemMediaTransportControlsSessionManager::RequestAsync().map_err(backend)?,
        )
        .await?;
        *self
            .manager
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(manager.clone());
        Ok(manager)
    }

    fn clear_manager(&self) {
        *self
            .manager
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    async fn find_session(
        &self,
        player: &str,
    ) -> Result<GlobalSystemMediaTransportControlsSession, MediaError> {
        let manager = self.manager().await?;
        ensure_winrt();
        let sessions = match sessions_of(&manager) {
            Ok(sessions) => sessions,
            Err(error) => {
                self.clear_manager();
                return Err(error);
            }
        };
        for session in sessions {
            let candidate = session
                .SourceAppUserModelId()
                .map(|id| id.to_string())
                .unwrap_or_default();
            if candidate == player {
                return Ok(session);
            }
        }
        Err(MediaError::PlayerNotFound(player.to_owned()))
    }
}

#[async_trait]
impl MediaProvider for SmtcMediaProvider {
    async fn snapshots(&self) -> Result<Vec<PlayerSnapshot>, MediaError> {
        let manager = self.manager().await?;
        ensure_winrt();
        let list = match sessions_of(&manager) {
            Ok(list) => list,
            Err(error) => {
                self.clear_manager();
                return Err(error);
            }
        };

        let mut snapshots = Vec::new();
        for session in list {
            if let Ok(snapshot) = snapshot_for(&session).await {
                snapshots.push(snapshot);
            }
        }
        Ok(snapshots)
    }

    async fn control(&self, player: &str, action: PlaybackAction) -> Result<(), MediaError> {
        let session = self.find_session(player).await?;
        ensure_winrt();
        let ok = match action {
            PlaybackAction::PlayPause => {
                wait(session.TryTogglePlayPauseAsync().map_err(backend)?).await?
            }
            PlaybackAction::Play => wait(session.TryPlayAsync().map_err(backend)?).await?,
            PlaybackAction::Pause => wait(session.TryPauseAsync().map_err(backend)?).await?,
            PlaybackAction::Seek { delta } => {
                let snapshot = snapshot_for(&session).await?;
                ensure_winrt();
                let base = snapshot.position.ok_or(MediaError::NotSupported("seek"))?;
                let target = (base + delta).max(0.0);
                wait(
                    session
                        .TryChangePlaybackPositionAsync(seconds_to_ticks(target))
                        .map_err(backend)?,
                )
                .await?
            }
            PlaybackAction::SeekTo { position, .. } => {
                wait(
                    session
                        .TryChangePlaybackPositionAsync(seconds_to_ticks(position.max(0.0)))
                        .map_err(backend)?,
                )
                .await?
            }
        };

        if ok {
            Ok(())
        } else {
            Err(MediaError::Backend(
                "the player refused the action".to_owned(),
            ))
        }
    }
}

async fn snapshot_for(
    session: &GlobalSystemMediaTransportControlsSession,
) -> Result<PlayerSnapshot, MediaError> {
    ensure_winrt();
    let player = session.SourceAppUserModelId().map_err(backend)?.to_string();
    let media = wait(session.TryGetMediaPropertiesAsync().map_err(backend)?).await?;
    ensure_winrt();
    let title = media
        .Title()
        .map(|value| value.to_string())
        .unwrap_or_default();
    let artist = media
        .Artist()
        .map(|value| value.to_string())
        .unwrap_or_default();
    let album = media
        .AlbumTitle()
        .map(|value| value.to_string())
        .unwrap_or_default();

    let playback = session.GetPlaybackInfo().map_err(backend)?;
    let status = playback.PlaybackStatus().unwrap_or_default();
    let playing = status == PlaybackStatus::Playing;
    let status_name = status_name(status);
    let controls = playback.Controls().ok();
    let can_play = controls
        .as_ref()
        .and_then(|controls| controls.IsPlayPauseToggleEnabled().ok())
        .unwrap_or(true);
    let can_position = controls
        .as_ref()
        .and_then(|controls| controls.IsPlaybackPositionEnabled().ok())
        .unwrap_or(false);
    let rate = playback
        .PlaybackRate()
        .ok()
        .and_then(|value| value.Value().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(1.0);

    let timeline = session.GetTimelineProperties().ok();
    let start = timeline
        .as_ref()
        .and_then(|timeline| timeline.StartTime().ok())
        .map(ticks_to_seconds);
    let end = timeline
        .as_ref()
        .and_then(|timeline| timeline.EndTime().ok())
        .map(ticks_to_seconds);
    let length = match (start, end) {
        (Some(start), Some(end)) if end > start => Some(round3(end - start)),
        _ => None,
    };
    let position = timeline
        .as_ref()
        .and_then(|timeline| timeline.Position().ok())
        .map(ticks_to_seconds)
        .map(|position| position - start.unwrap_or(0.0))
        .map(|position| {
            if playing {
                position
                    + age_since(
                        timeline
                            .as_ref()
                            .and_then(|timeline| timeline.LastUpdatedTime().ok()),
                    ) * rate
            } else {
                position
            }
        })
        .map(|position| round3(position.max(0.0)));
    let can_seek = can_position && position.is_some() && length.is_some_and(|value| value > 0.0);

    Ok(PlayerSnapshot {
        label: label_for(&player),
        track_id: format!("{player}|{title}|{artist}"),
        player,
        title,
        artist,
        album,
        length,
        position,
        playing,
        status: status_name,
        can_seek,
        can_play,
        rate: round3(rate),
        sampled_at: Instant::now(),
    })
}

async fn wait<T, F>(operation: F) -> Result<T, MediaError>
where
    F: IntoFuture<Output = windows::core::Result<T>>,
{
    tokio::time::timeout(SMTC_TIMEOUT, operation.into_future())
        .await
        .map_err(|_| MediaError::Backend("SMTC operation timed out".to_owned()))?
        .map_err(backend)
}

fn backend(error: windows::core::Error) -> MediaError {
    MediaError::Backend(error.to_string())
}

fn ensure_winrt() {
    unsafe {
        let _ = windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
        );
    }
}

fn sessions_of(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
) -> Result<Vec<GlobalSystemMediaTransportControlsSession>, MediaError> {
    let sessions = manager.GetSessions().map_err(backend)?;
    let size = sessions.Size().map_err(backend)?;
    let mut list = Vec::with_capacity(size as usize);
    for index in 0..size {
        list.push(sessions.GetAt(index).map_err(backend)?);
    }
    Ok(list)
}

fn status_name(status: PlaybackStatus) -> String {
    match status {
        PlaybackStatus::Closed => "Closed",
        PlaybackStatus::Opened => "Opened",
        PlaybackStatus::Changing => "Changing",
        PlaybackStatus::Stopped => "Stopped",
        PlaybackStatus::Playing => "Playing",
        PlaybackStatus::Paused => "Paused",
        _ => "Unknown",
    }
    .to_owned()
}

fn ticks_to_seconds(span: TimeSpan) -> f64 {
    span.Duration as f64 / TICKS_PER_SECOND
}

fn seconds_to_ticks(seconds: f64) -> i64 {
    (seconds.max(0.0) * TICKS_PER_SECOND).round() as i64
}

fn age_since(updated: Option<DateTime>) -> f64 {
    let Some(updated) = updated else {
        return 0.0;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as i64 / 100)
        .unwrap_or(0)
        + WINDOWS_EPOCH_OFFSET_TICKS;
    ((now - updated.UniversalTime).max(0) as f64) / TICKS_PER_SECOND
}

fn label_for(aumid: &str) -> String {
    let value = aumid.trim();
    let lower = value.to_lowercase();
    for (needle, label) in KNOWN_APPS {
        if lower.contains(needle) {
            return (*label).to_owned();
        }
    }

    let name = value
        .split('!')
        .next_back()
        .unwrap_or(value)
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(value);
    let name = name
        .strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".EXE"))
        .unwrap_or(name);
    if name.is_empty() {
        "Player".to_owned()
    } else {
        name.to_owned()
    }
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_known_apps_and_falls_back_to_the_executable() {
        assert_eq!(label_for("Spotify.exe"), "Spotify");
        assert_eq!(
            label_for("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"),
            "Media Player"
        );
        assert_eq!(label_for(r"Program Files\VLC\vlc.exe"), "VLC");
        assert_eq!(label_for(r"Tools\MyPlayer.exe"), "MyPlayer");
    }

    #[test]
    fn converts_ticks_and_clamps_negative_positions() {
        assert_eq!(seconds_to_ticks(1.5), 15_000_000);
        assert_eq!(
            ticks_to_seconds(TimeSpan {
                Duration: 15_000_000
            }),
            1.5
        );
        assert_eq!(seconds_to_ticks(-2.0), 0);
    }
}
