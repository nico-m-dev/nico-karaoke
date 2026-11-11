use std::sync::LazyLock;

use regex::Regex;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

static DECORATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[\(\[](?:[^)\]]*(?:official|video|audio|lyric|lyrics|visuali[sz]er|remaster|explicit|hd|hq|4k)[^)\]]*)[\)\]]",
    )
    .expect("decoration regex is valid")
});
static FEAT_PARENTHESIS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[\(\[]\s*(?:feat|ft|featuring)\.?\s[^)\]]*[\)\]]")
        .expect("feature regex is valid")
});
static FEAT_TAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s+(?:feat|ft|featuring)\.?\s.*$").expect("feature tail regex is valid")
});
static CHANNEL_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\s*[-–—]\s*|\s*)(?:topic|vevo)\s*$").expect("channel regex is valid")
});
static TITLE_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(.+?)\s+[-–—]\s+(.+)$").expect("title separator regex is valid")
});

/// Remove common video/channel decorations from a title.
pub fn clean(text: &str) -> String {
    let text = DECORATION.replace_all(text, "");
    let text = FEAT_PARENTHESIS.replace_all(&text, "");
    let text = FEAT_TAIL.replace_all(&text, "");
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.trim_matches([' ', '-', '–', '—']).to_owned()
}

/// Remove a trailing `- Topic` or `VEVO` suffix from an artist/channel name.
pub fn strip_channel(artist: &str) -> String {
    CHANNEL_SUFFIX.replace(artist, "").trim().to_owned()
}

/// Normalise text for comparisons and record keys. Accents are folded, punctuation and case
/// are ignored. Non-Latin characters remain because they are alphanumeric in Unicode.
pub fn normalize(text: &str) -> String {
    text.nfkd()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Split `Artist - Title` when the metadata channel is clearly not the real artist.
pub fn split_title(title: &str, artist: &str) -> (String, String) {
    let raw_artist = clean(artist);
    let normalised_artist = normalize(&raw_artist);
    let channelish = CHANNEL_SUFFIX.is_match(artist)
        || normalised_artist.ends_with("topic")
        || normalised_artist.ends_with("vevo");

    let cleaned_artist = strip_channel(&raw_artist);
    let cleaned_title = clean(title);

    let Some(captures) = TITLE_SEPARATOR.captures(&cleaned_title) else {
        return (cleaned_title, cleaned_artist);
    };
    let Some(left) = captures.get(1) else {
        return (cleaned_title, cleaned_artist);
    };
    let Some(right) = captures.get(2) else {
        return (cleaned_title, cleaned_artist);
    };

    let left = left.as_str().trim();
    let right = right.as_str().trim();
    let artist_key = normalize(&cleaned_artist);
    let left_key = normalize(left);

    if channelish
        || artist_key.is_empty()
        || left_key.contains(&artist_key)
        || artist_key.contains(&left_key)
    {
        (right.to_owned(), strip_channel(left))
    } else {
        (cleaned_title, cleaned_artist)
    }
}

/// Stable key for a track. The version prefix allows a future algorithm change without
/// silently mixing old and new records.
pub fn record_key(title: &str, artist: &str) -> String {
    let (title, artist) = split_title(title, artist);
    let raw = format!("v1|{}|{}", normalize(&artist), normalize(&title));
    let digest = Sha256::digest(raw.as_bytes());
    hex::encode(digest)
}

/// Human-readable file stem. Always ASCII and bounded so the library stays portable.
pub fn slugify(value: &str) -> String {
    let mut output = String::new();
    let mut previous_dash = false;

    for character in value.nfkd().filter(char::is_ascii) {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash && !output.is_empty() {
            output.push('-');
            previous_dash = true;
        }
    }

    let output = output.trim_matches('-');
    let truncated: String = output.chars().take(60).collect();
    truncated.trim_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_video_decorations_and_features() {
        assert_eq!(
            clean("Song (Official Video) [Lyrics] feat. Someone"),
            "Song"
        );
        assert_eq!(clean("Song [4K Remaster]"), "Song");
        assert_eq!(clean("  Song   Title  "), "Song Title");
    }

    #[test]
    fn strips_channel_suffixes() {
        assert_eq!(strip_channel("Artist - Topic"), "Artist");
        assert_eq!(strip_channel("Artist VEVO"), "Artist");
        assert_eq!(strip_channel("Artist"), "Artist");
    }

    #[test]
    fn splits_artist_and_title_for_channel_style_metadata() {
        assert_eq!(
            split_title("Real Artist - Real Song", "Real Artist - Topic"),
            ("Real Song".to_owned(), "Real Artist".to_owned())
        );
        assert_eq!(
            split_title("Artist - Song", "Artist"),
            ("Song".to_owned(), "Artist".to_owned())
        );
    }

    #[test]
    fn keeps_a_dash_in_a_normal_title() {
        assert_eq!(
            split_title("Song - Live", "Someone Else"),
            ("Song - Live".to_owned(), "Someone Else".to_owned())
        );
    }

    #[test]
    fn normalises_accents_and_punctuation() {
        assert_eq!(normalize("Beyoncé!"), "beyonce");
        assert_eq!(normalize("Hello, World"), "helloworld");
    }

    #[test]
    fn slugifies_to_portable_ascii() {
        assert_eq!(slugify("Café del Mar"), "cafe-del-mar");
        assert_eq!(slugify("  Hello / World  "), "hello-world");
        assert!(slugify(&"a".repeat(100)).len() <= 60);
    }

    #[test]
    fn creates_stable_and_distinct_keys() {
        let first = record_key("Song", "Artist");
        let second = record_key("  Song  ", "Artist");
        let other = record_key("Other Song", "Artist");

        assert_eq!(first, second);
        assert_ne!(first, other);
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
    }

    #[test]
    fn keys_ignore_channel_noise() {
        assert_eq!(
            record_key("Real Song", "Real Artist - Topic"),
            record_key("Real Artist - Real Song", "Real Artist")
        );
    }
}
