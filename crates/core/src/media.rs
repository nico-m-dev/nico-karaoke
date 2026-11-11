use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

use crate::title::normalize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerInfo {
    pub player: String,
    pub label: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSnapshot {
    pub player: String,
    pub label: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_id: String,
    pub length: Option<f64>,
    pub position: Option<f64>,
    pub playing: bool,
    pub status: String,
    pub can_seek: bool,
    pub can_play: bool,
    pub rate: f64,
    pub sampled_at: Instant,
}

impl PlayerSnapshot {
    pub fn new(
        player: impl Into<String>,
        title: impl Into<String>,
        artist: impl Into<String>,
    ) -> Self {
        let player = player.into();
        Self {
            label: player.clone(),
            player,
            title: title.into(),
            artist: artist.into(),
            album: String::new(),
            track_id: String::new(),
            length: None,
            position: None,
            playing: false,
            status: String::new(),
            can_seek: false,
            can_play: true,
            rate: 1.0,
            sampled_at: Instant::now(),
        }
    }

    pub fn info(&self) -> PlayerInfo {
        PlayerInfo {
            player: self.player.clone(),
            label: self.label.clone(),
            title: self.title.clone(),
            artist: self.artist.clone(),
            playing: self.playing,
        }
    }

    /// Normalised identity used to detect a real track change, not a position update.
    pub fn signature(&self) -> String {
        format!("{}|{}", normalize(&self.artist), normalize(&self.title))
    }

    /// Interpolate the playback position between provider polls. A paused player keeps its
    /// last reported position; a playing player advances at its playback rate.
    pub fn position_at(&self, now: Instant) -> Option<f64> {
        let position = self.position?;
        if !self.playing {
            return Some(position.max(0.0));
        }

        let age = now.saturating_duration_since(self.sampled_at).as_secs_f64();
        let rate = if self.rate.is_finite() && self.rate > 0.0 {
            self.rate
        } else {
            1.0
        };
        let value = position.max(0.0) + age * rate;
        Some(
            self.length
                .filter(|length| length.is_finite() && *length > 0.0)
                .map_or(value, |length| value.min(length)),
        )
    }
}

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("no media player is available")]
    NoPlayer,
    #[error("media backend unavailable: {0}")]
    Unavailable(String),
    #[error("player not found: {0}")]
    PlayerNotFound(String),
    #[error("this player does not support {0}")]
    NotSupported(&'static str),
    #[error("media backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackAction {
    PlayPause,
    Play,
    Pause,
    Seek { delta: f64 },
    SeekTo { track_id: String, position: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackControl {
    PlayPause,
    Play,
    Pause,
    SeekRelative { delta: f64 },
    SeekTo { position: f64 },
}

#[async_trait]
pub trait MediaProvider: Send + Sync {
    async fn snapshots(&self) -> Result<Vec<PlayerSnapshot>, MediaError>;
    async fn control(&self, player: &str, action: PlaybackAction) -> Result<(), MediaError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaState {
    pub players: Vec<PlayerInfo>,
    pub snapshot: Option<PlayerSnapshot>,
    pub chosen: Option<String>,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaUpdate {
    pub state: MediaState,
    pub track_changed: bool,
}

/// Selects the followed player and tracks a generation that a background lookup can use to
/// discard late results when the track changes.
#[derive(Debug, Default)]
pub struct MediaTracker {
    players: Vec<PlayerInfo>,
    snapshot: Option<PlayerSnapshot>,
    chosen: Option<String>,
    signature: Option<String>,
    generation: u64,
}

impl MediaTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn poll(&mut self, provider: &dyn MediaProvider) -> Result<MediaUpdate, MediaError> {
        let snapshots = provider.snapshots().await?;
        Ok(self.update(snapshots))
    }

    pub fn update(&mut self, snapshots: Vec<PlayerSnapshot>) -> MediaUpdate {
        self.players = snapshots.iter().map(PlayerSnapshot::info).collect();

        let chosen = self.pick(&snapshots).cloned();
        self.chosen = chosen.as_ref().map(|snapshot| snapshot.player.clone());
        self.snapshot = chosen;

        let signature = self.snapshot.as_ref().map(PlayerSnapshot::signature);
        let track_changed = signature != self.signature;
        if track_changed {
            self.generation = self.generation.wrapping_add(1);
        }
        self.signature = signature;

        MediaUpdate {
            state: self.state(),
            track_changed,
        }
    }

    fn pick<'a>(&self, snapshots: &'a [PlayerSnapshot]) -> Option<&'a PlayerSnapshot> {
        if let Some(chosen) = &self.chosen {
            if let Some(found) = snapshots
                .iter()
                .find(|snapshot| snapshot.player == *chosen && !snapshot.title.is_empty())
            {
                return Some(found);
            }
        }

        snapshots
            .iter()
            .find(|snapshot| snapshot.playing && !snapshot.title.is_empty())
            .or_else(|| snapshots.iter().find(|snapshot| !snapshot.title.is_empty()))
    }

    pub fn choose(&mut self, player: &str) -> bool {
        let known = self
            .players
            .iter()
            .any(|candidate| candidate.player == player)
            || self
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.player == player);
        if known {
            self.chosen = Some(player.to_owned());
        }
        known
    }

    pub fn state(&self) -> MediaState {
        MediaState {
            players: self.players.clone(),
            snapshot: self.snapshot.clone(),
            chosen: self.chosen.clone(),
            generation: self.generation,
        }
    }

    pub fn snapshot(&self) -> Option<&PlayerSnapshot> {
        self.snapshot.as_ref()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub async fn control(
        &self,
        provider: &dyn MediaProvider,
        control: PlaybackControl,
    ) -> Result<(), MediaError> {
        let snapshot = self.snapshot.as_ref().ok_or(MediaError::NoPlayer)?;
        let action = match control {
            PlaybackControl::PlayPause => PlaybackAction::PlayPause,
            PlaybackControl::Play => PlaybackAction::Play,
            PlaybackControl::Pause => PlaybackAction::Pause,
            PlaybackControl::SeekRelative { delta } => {
                if !snapshot.can_seek {
                    return Err(MediaError::NotSupported("seek"));
                }
                PlaybackAction::Seek { delta }
            }
            PlaybackControl::SeekTo { position } => {
                if !snapshot.can_seek {
                    return Err(MediaError::NotSupported("seek"));
                }
                if snapshot.track_id.is_empty() {
                    return Err(MediaError::NotSupported("absolute seek"));
                }
                PlaybackAction::SeekTo {
                    track_id: snapshot.track_id.clone(),
                    position,
                }
            }
        };
        provider.control(&snapshot.player, action).await
    }
}

/// In-memory provider for tests and local UI development.
#[derive(Default)]
pub struct FakeMediaProvider {
    snapshots: Mutex<Vec<PlayerSnapshot>>,
    actions: Mutex<Vec<(String, PlaybackAction)>>,
}

impl FakeMediaProvider {
    pub fn new(snapshots: Vec<PlayerSnapshot>) -> Self {
        Self {
            snapshots: Mutex::new(snapshots),
            actions: Mutex::new(Vec::new()),
        }
    }

    pub fn set_snapshots(&self, snapshots: Vec<PlayerSnapshot>) {
        *self
            .snapshots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshots;
    }

    pub fn actions(&self) -> Vec<(String, PlaybackAction)> {
        self.actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[async_trait]
impl MediaProvider for FakeMediaProvider {
    async fn snapshots(&self) -> Result<Vec<PlayerSnapshot>, MediaError> {
        Ok(self
            .snapshots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone())
    }

    async fn control(&self, player: &str, action: PlaybackAction) -> Result<(), MediaError> {
        let known = self
            .snapshots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .any(|snapshot| snapshot.player == player);
        if !known {
            return Err(MediaError::PlayerNotFound(player.to_owned()));
        }
        self.actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((player.to_owned(), action));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn snapshot(player: &str, title: &str, artist: &str) -> PlayerSnapshot {
        PlayerSnapshot::new(player, title, artist)
    }

    #[tokio::test]
    async fn chooses_the_playing_player_and_falls_back_to_any_titled_player() {
        let mut playing = snapshot("org.mpris.MediaPlayer2.spotify", "Song", "Artist");
        playing.playing = true;
        let idle = snapshot("org.mpris.MediaPlayer2.browser", "Other", "Artist");
        let mut tracker = MediaTracker::new();

        let update = tracker.update(vec![idle.clone(), playing.clone()]);
        assert_eq!(
            update.state.chosen.as_deref(),
            Some(playing.player.as_str())
        );
        assert_eq!(update.state.snapshot, Some(playing.clone()));

        let update = tracker.update(vec![idle.clone()]);
        assert_eq!(update.state.chosen.as_deref(), Some(idle.player.as_str()));
        assert!(update.track_changed);
    }

    #[tokio::test]
    async fn keeps_a_manually_chosen_player_until_it_disappears() {
        let mut first = snapshot("player-a", "Song A", "Artist");
        first.playing = true;
        let mut second = snapshot("player-b", "Song B", "Artist");
        second.playing = true;
        let mut tracker = MediaTracker::new();
        tracker.update(vec![first.clone(), second.clone()]);

        assert!(tracker.choose("player-b"));
        let update = tracker.update(vec![first, second.clone()]);
        assert_eq!(update.state.chosen.as_deref(), Some("player-b"));

        let fallback = tracker.update(vec![snapshot("player-a", "Song A", "Artist")]);
        assert_eq!(fallback.state.chosen.as_deref(), Some("player-a"));
        assert!(fallback.track_changed);
    }

    #[tokio::test]
    async fn reports_a_new_generation_only_when_the_track_changes() {
        let mut tracker = MediaTracker::new();
        let first = tracker.update(vec![snapshot("player", "Song", "Artist")]);
        let generation = first.state.generation;
        assert!(first.track_changed);

        let mut same_track = snapshot("player", "Song", "Artist");
        same_track.position = Some(12.0);
        let update = tracker.update(vec![same_track]);
        assert!(!update.track_changed);
        assert_eq!(update.state.generation, generation);

        let update = tracker.update(vec![snapshot("player", "Other Song", "Artist")]);
        assert!(update.track_changed);
        assert!(update.state.generation > generation);
    }

    #[tokio::test]
    async fn clears_the_snapshot_when_no_player_has_a_title() {
        let mut tracker = MediaTracker::new();
        tracker.update(vec![snapshot("player", "Song", "Artist")]);
        let update = tracker.update(vec![snapshot("player", "", "")]);

        assert!(update.state.snapshot.is_none());
        assert!(update.state.chosen.is_none());
        assert!(update.track_changed);
    }

    #[test]
    fn interpolates_position_only_while_playing() {
        let now = Instant::now();
        let mut item = snapshot("player", "Song", "Artist");
        item.position = Some(10.0);
        item.length = Some(30.0);
        item.playing = true;
        item.sampled_at = now - Duration::from_secs(2);

        assert_eq!(item.position_at(now), Some(12.0));

        item.playing = false;
        assert_eq!(item.position_at(now), Some(10.0));

        item.playing = true;
        item.length = Some(11.0);
        assert_eq!(item.position_at(now), Some(11.0));
        assert_eq!(item.position_at(now - Duration::from_secs(5)), Some(10.0));
    }

    #[tokio::test]
    async fn controls_the_followed_player_and_rejects_unsupported_actions() {
        let mut item = snapshot("player", "Song", "Artist");
        item.can_seek = true;
        item.track_id = "/track/1".to_owned();
        let provider = FakeMediaProvider::new(vec![item]);
        let mut tracker = MediaTracker::new();
        tracker.update(provider.snapshots().await.expect("snapshots"));

        tracker
            .control(&provider, PlaybackControl::PlayPause)
            .await
            .expect("playpause works");
        tracker
            .control(&provider, PlaybackControl::SeekTo { position: 12.5 })
            .await
            .expect("absolute seek works");

        let actions = provider.actions();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].1, PlaybackAction::PlayPause);
        assert_eq!(
            actions[1].1,
            PlaybackAction::SeekTo {
                track_id: "/track/1".to_owned(),
                position: 12.5
            }
        );
    }

    #[tokio::test]
    async fn refuses_absolute_seek_without_track_identity() {
        let mut item = snapshot("player", "Song", "Artist");
        item.can_seek = true;
        let provider = FakeMediaProvider::new(vec![item]);
        let mut tracker = MediaTracker::new();
        tracker.update(provider.snapshots().await.expect("snapshots"));

        let error = tracker
            .control(&provider, PlaybackControl::SeekTo { position: 1.0 })
            .await
            .expect_err("absolute seek is rejected");
        assert!(matches!(error, MediaError::NotSupported("absolute seek")));
    }

    #[tokio::test]
    async fn fake_provider_records_invalid_player_requests() {
        let provider = FakeMediaProvider::default();
        let error = provider
            .control("missing", PlaybackAction::PlayPause)
            .await
            .expect_err("missing player is rejected");
        assert!(matches!(error, MediaError::PlayerNotFound(_)));
    }
}
