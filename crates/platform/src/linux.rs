use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;
use nico_karaoke_core::{MediaError, MediaProvider, PlaybackAction, PlayerSnapshot};
use zbus::Proxy;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const ROOT_IFACE: &str = "org.mpris.MediaPlayer2";
const DBUS_IFACE: &str = "org.freedesktop.DBus";

pub struct LinuxMediaProvider {
    connection: zbus::Connection,
}

impl LinuxMediaProvider {
    pub async fn new() -> Result<Self, MediaError> {
        let connection = zbus::Connection::session()
            .await
            .map_err(|error| MediaError::Unavailable(error.to_string()))?;
        Ok(Self { connection })
    }

    async fn player_names(&self) -> Result<Vec<String>, MediaError> {
        let proxy = Proxy::new(
            &self.connection,
            DBUS_IFACE,
            "/org/freedesktop/DBus",
            DBUS_IFACE,
        )
        .await
        .map_err(backend)?;
        let names: Vec<String> = proxy
            .call("ListNames", &())
            .await
            .map_err(|error| MediaError::Unavailable(error.to_string()))?;
        let mut players: Vec<String> = names
            .into_iter()
            .filter(|name| name.starts_with(MPRIS_PREFIX))
            .collect();
        players.sort();
        Ok(players)
    }

    async fn snapshot_for(&self, name: &str) -> Result<Option<PlayerSnapshot>, MediaError> {
        let player = Proxy::new(&self.connection, name, OBJECT_PATH, PLAYER_IFACE)
            .await
            .map_err(backend)?;

        let status: String = player
            .get_property("PlaybackStatus")
            .await
            .unwrap_or_default();
        let metadata: HashMap<String, OwnedValue> =
            player.get_property("Metadata").await.unwrap_or_default();
        let position_micros: Option<i64> = player.get_property("Position").await.ok();
        let rate: Option<f64> = player.get_property("Rate").await.ok();
        let can_seek: bool = player.get_property("CanSeek").await.unwrap_or(false);
        let can_play: bool = player.get_property("CanPlay").await.unwrap_or(false);

        let identity: Option<String> =
            match Proxy::new(&self.connection, name, OBJECT_PATH, ROOT_IFACE).await {
                Ok(proxy) => proxy.get_property("Identity").await.ok(),
                Err(_) => None,
            };
        let label = identity
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| fallback_label(name));

        Ok(Some(PlayerSnapshot {
            player: name.to_owned(),
            label,
            title: metadata_text(&metadata, "xesam:title"),
            artist: metadata_text(&metadata, "xesam:artist"),
            album: metadata_text(&metadata, "xesam:album"),
            track_id: metadata_track_id(&metadata),
            length: metadata_length(&metadata),
            position: position_micros
                .filter(|value| *value >= 0)
                .map(|value| round3(value as f64 / 1_000_000.0)),
            playing: status == "Playing",
            status,
            can_seek,
            can_play,
            rate: rate
                .filter(|value| value.is_finite() && *value > 0.0)
                .map(round3)
                .unwrap_or(1.0),
            sampled_at: Instant::now(),
        }))
    }
}

#[async_trait]
impl MediaProvider for LinuxMediaProvider {
    async fn snapshots(&self) -> Result<Vec<PlayerSnapshot>, MediaError> {
        let mut snapshots = Vec::new();
        for name in self.player_names().await? {
            match self.snapshot_for(&name).await {
                Ok(Some(snapshot)) => snapshots.push(snapshot),
                Ok(None) => {}
                Err(_) => {
                    // A player can disappear between ListNames and GetAll.
                }
            }
        }
        Ok(snapshots)
    }

    async fn control(&self, player: &str, action: PlaybackAction) -> Result<(), MediaError> {
        let proxy = Proxy::new(&self.connection, player, OBJECT_PATH, PLAYER_IFACE)
            .await
            .map_err(backend)?;

        match action {
            PlaybackAction::PlayPause => {
                call_unit(&proxy, "PlayPause").await?;
            }
            PlaybackAction::Play => {
                call_unit(&proxy, "Play").await?;
            }
            PlaybackAction::Pause => {
                call_unit(&proxy, "Pause").await?;
            }
            PlaybackAction::Seek { delta } => {
                let micros = (delta * 1_000_000.0).round() as i64;
                proxy
                    .call::<_, _, ()>("Seek", &(micros,))
                    .await
                    .map_err(backend)?;
            }
            PlaybackAction::SeekTo { track_id, position } => {
                let track_id = OwnedObjectPath::try_from(track_id.as_str())
                    .map_err(|error| MediaError::Backend(error.to_string()))?;
                let micros = (position.max(0.0) * 1_000_000.0).round() as i64;
                proxy
                    .call::<_, _, ()>("SetPosition", &(track_id, micros))
                    .await
                    .map_err(backend)?;
            }
        }
        Ok(())
    }
}

async fn call_unit(proxy: &Proxy<'_>, method: &str) -> Result<(), MediaError> {
    proxy.call::<_, _, ()>(method, &()).await.map_err(backend)
}

fn backend(error: zbus::Error) -> MediaError {
    MediaError::Backend(error.to_string())
}

fn metadata_text(metadata: &HashMap<String, OwnedValue>, key: &str) -> String {
    let Some(value) = metadata.get(key) else {
        return String::new();
    };
    if let Ok(text) = String::try_from(value.clone()) {
        return text.trim().to_owned();
    }
    if let Ok(items) = Vec::<String>::try_from(value.clone()) {
        return items.join(", ");
    }
    String::new()
}

fn metadata_length(metadata: &HashMap<String, OwnedValue>) -> Option<f64> {
    let value = metadata.get("mpris:length")?;
    let micros = i64::try_from(value)
        .ok()
        .or_else(|| u64::try_from(value).ok().map(|value| value as i64))?;
    (micros > 0).then(|| round3(micros as f64 / 1_000_000.0))
}

fn metadata_track_id(metadata: &HashMap<String, OwnedValue>) -> String {
    metadata
        .get("mpris:trackid")
        .and_then(|value| OwnedObjectPath::try_from(value.clone()).ok())
        .map(|path| path.to_string())
        .unwrap_or_default()
}

fn fallback_label(name: &str) -> String {
    let rest = name.strip_prefix(MPRIS_PREFIX).unwrap_or(name);
    let first = rest
        .split(['.', '!'])
        .next()
        .unwrap_or(rest)
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(rest);
    if first.is_empty() {
        name.to_owned()
    } else {
        capitalize(first)
    }
}

fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_uppercase().chain(characters).collect()
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    #[test]
    fn builds_labels_from_well_known_names() {
        assert_eq!(fallback_label("org.mpris.MediaPlayer2.spotify"), "Spotify");
        assert_eq!(
            fallback_label("org.mpris.MediaPlayer2.firefox.instance42"),
            "Firefox"
        );
        assert_eq!(fallback_label("org.mpris.MediaPlayer2.brave"), "Brave");
    }

    #[test]
    fn reads_metadata_strings_and_lengths() {
        let metadata = HashMap::from([
            (
                "xesam:title".to_owned(),
                OwnedValue::try_from(Value::from("Song title")).expect("string value"),
            ),
            ("mpris:length".to_owned(), OwnedValue::from(180_500_000i64)),
        ]);

        assert_eq!(metadata_text(&metadata, "xesam:title"), "Song title");
        assert_eq!(metadata_text(&metadata, "xesam:artist"), "");
        assert_eq!(metadata_length(&metadata), Some(180.5));
    }

    #[test]
    fn reads_track_ids() {
        let path = zbus::zvariant::ObjectPath::try_from("/org/mpris/MediaPlayer2/Track/1")
            .expect("object path");
        let metadata = HashMap::from([(
            "mpris:trackid".to_owned(),
            OwnedValue::try_from(Value::ObjectPath(path)).expect("object path value"),
        )]);

        assert_eq!(
            metadata_track_id(&metadata),
            "/org/mpris/MediaPlayer2/Track/1"
        );
    }

    #[test]
    fn ignores_invalid_lengths() {
        let metadata = HashMap::from([("mpris:length".to_owned(), OwnedValue::from(0i64))]);
        assert_eq!(metadata_length(&metadata), None);
    }
}
