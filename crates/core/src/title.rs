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
/// A right-hand side that names a version, not a song. Parenthetical versions stay apart
/// through normalisation instead of this list.
static VERSION_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(?:live|remix|rmx|acoustic|instrumental|demo|edit|version|extended|reprise|cover|nightcore|slowed|sped\s*up|karaoke|remaster(?:ed)?)$",
    )
    .expect("version suffix regex is valid")
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

/// The artist and title hidden in a decorated video title such as
/// `Artist - Song (Official Video)`. Only titles with a video/lyric decoration or a featured
/// artist qualify: without one, `Song - Live` is far more likely the whole title than a credit.
pub(crate) fn decorated_credit(title: &str) -> Option<(String, String)> {
    let decorated =
        DECORATION.is_match(title) || FEAT_PARENTHESIS.is_match(title) || FEAT_TAIL.is_match(title);
    if !decorated {
        return None;
    }

    let cleaned = clean(title);
    let captures = TITLE_SEPARATOR.captures(&cleaned)?;
    let left = captures.get(1)?.as_str().trim();
    let right = captures.get(2)?.as_str().trim();
    if left.is_empty() || right.is_empty() || VERSION_SUFFIX.is_match(right) {
        return None;
    }
    Some((left.to_owned(), right.to_owned()))
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

    // A one-character left side ("A - B", "O - ...") would match almost any artist name, so
    // only treat a longer left side as the artist. Channel-style titles still split without an
    // artist, because that is the only way to recover the artist from the metadata.
    let left_matches_artist = left_key.contains(&artist_key)
        || (left_key.chars().count() >= 2 && artist_key.contains(&left_key));

    if channelish || artist_key.is_empty() || left_matches_artist {
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
    fn recovers_a_credit_from_a_decorated_video_title() {
        assert_eq!(
            decorated_credit("Combichrist - Feraline (Official Lyric Video)"),
            Some(("Combichrist".to_owned(), "Feraline".to_owned()))
        );
        assert_eq!(
            decorated_credit("Ghost B.C. - Year Zero (AUDIO)"),
            Some(("Ghost B.C.".to_owned(), "Year Zero".to_owned()))
        );
        assert_eq!(decorated_credit("Song - Live"), None);
        assert_eq!(decorated_credit("Song - Live (Official Video)"), None);
        assert_eq!(decorated_credit("Song"), None);
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
    fn does_not_split_a_one_letter_title_fragment_that_matches_the_artist() {
        assert_eq!(
            split_title("A - B", "Cat Stevens"),
            ("A - B".to_owned(), "Cat Stevens".to_owned())
        );
        assert_eq!(
            split_title("O - Burning", "Foo Fighters"),
            ("O - Burning".to_owned(), "Foo Fighters".to_owned())
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
