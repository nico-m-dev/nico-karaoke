use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]").expect("timestamp regex is valid")
});
static METADATA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\[(ar|ti|al|by|offset|length|re|ve|tool):(.*)\]$")
        .expect("metadata regex is valid")
});
static WORD_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<\d{1,3}:\d{1,2}(?:[.:]\d{1,3})?>").expect("word-tag regex is valid")
});
static LEGACY_END: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*\[end:\s*(\d{1,3}):(\d{1,2}(?:[.:]\d{1,3})?)\s*\]\s*$")
        .expect("legacy end-tag regex is valid")
});

/// One displayed lyric line.
///
/// `t` is the start time in seconds. Plain-text lines keep `t = None`. `end` only exists for
/// legacy `[end:mm:ss.xx]` tags; new files use `♪` music markers instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricLine {
    pub t: Option<f64>,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
}

impl LyricLine {
    pub fn timed(time: f64, text: impl Into<String>) -> Self {
        Self {
            t: Some(round_millis(time)),
            text: text.into(),
            end: None,
        }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            t: None,
            text: text.into(),
            end: None,
        }
    }

    pub fn is_music(&self) -> bool {
        is_music_marker(&self.text)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LrcDocument {
    pub synced: bool,
    pub lines: Vec<LyricLine>,
    /// Metadata tags such as `[ar:]`, `[ti:]`, and `[al:]`. `[offset:]` is deliberately not
    /// kept because its value is applied to the timestamps while parsing.
    #[serde(default)]
    pub metadata: Vec<String>,
    /// Untimed, non-metadata lines from a mixed document. They are preserved on save but not
    /// shown as timed lyrics.
    #[serde(default)]
    pub extras: Vec<String>,
}

impl LrcDocument {
    pub fn parse(text: &str) -> Self {
        parse_lrc(text)
    }

    pub fn to_lrc(&self) -> String {
        to_lrc(&self.metadata, &self.extras, &self.lines)
    }

    pub fn line_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| !line.text.trim().is_empty())
            .count()
    }

    pub fn has_ends(&self) -> bool {
        self.lines.iter().any(|line| line.end.is_some())
    }
}

/// Parse LRC or plain lyrics.
///
/// The parser accepts metadata tags, multiple timestamps per line, `[offset:]`, enhanced-LRC
/// word tags (stripped for now), and legacy `[end:]` tags. A document with at least one timed
/// line is considered synchronised.
pub fn parse_lrc(text: &str) -> LrcDocument {
    let mut offset = 0.0;
    let mut timed = Vec::new();
    let mut plain = Vec::new();
    let mut metadata = Vec::new();

    for raw in text.lines() {
        let line = raw.trim_start_matches('\u{feff}').trim();
        if line.is_empty() {
            continue;
        }

        if let Some(captures) = METADATA.captures(line) {
            if captures
                .get(1)
                .is_some_and(|kind| kind.as_str().eq_ignore_ascii_case("offset"))
            {
                if let Some(value) = captures.get(2) {
                    if let Ok(milliseconds) = value.as_str().trim().parse::<i64>() {
                        offset = milliseconds as f64 / 1000.0;
                    }
                }
            } else {
                metadata.push(line.to_owned());
            }
            continue;
        }

        let mut body = line.to_owned();
        let mut end = None;
        if let Some(captures) = LEGACY_END.captures(&body) {
            if let (Some(minutes), Some(seconds)) = (captures.get(1), captures.get(2)) {
                end = parse_timestamp(minutes.as_str(), seconds.as_str());
            }
            if let Some(whole_match) = captures.get(0) {
                body.truncate(whole_match.start());
                body = body.trim_end().to_owned();
            }
        }

        let mut times = Vec::new();
        let mut rest = body.as_str();
        while let Some(captures) = TIMESTAMP.captures(rest) {
            let Some(whole_match) = captures.get(0) else {
                break;
            };
            if whole_match.start() != 0 {
                break;
            }

            let minutes = captures
                .get(1)
                .map(|value| value.as_str())
                .unwrap_or_default();
            let mut seconds = captures
                .get(2)
                .map(|value| value.as_str().to_owned())
                .unwrap_or_default();
            if let Some(fraction) = captures.get(3) {
                seconds.push('.');
                seconds.push_str(fraction.as_str());
            }
            if let Some(time) = parse_timestamp(minutes, &seconds) {
                times.push(time);
            }
            rest = rest[whole_match.end()..].trim_start_matches([' ', '\t']);
        }

        let body = WORD_TAG.replace_all(rest, "").trim().to_owned();
        if times.is_empty() {
            if !body.is_empty() {
                plain.push(LyricLine::plain(body));
            }
            continue;
        }

        for time in times {
            let start = round_millis(time);
            let line_end = end.filter(|value| *value > time).map(round_millis);
            timed.push(LyricLine {
                t: Some(start),
                text: body.clone(),
                end: line_end,
            });
        }
    }

    if offset != 0.0 {
        for line in &mut timed {
            if let Some(time) = line.t {
                line.t = Some(round_millis((time + offset).max(0.0)));
            }
            if let Some(end) = line.end {
                line.end = Some(round_millis((end + offset).max(0.0)));
            }
        }
    }

    if timed.is_empty() {
        LrcDocument {
            synced: false,
            lines: plain,
            metadata,
            extras: Vec::new(),
        }
    } else {
        timed.sort_by(|left, right| {
            left.t
                .unwrap_or_default()
                .total_cmp(&right.t.unwrap_or_default())
        });
        // Untimed lines in a mixed document are not displayed as lyrics, but they are kept so
        // a Studio save does not silently delete headers or credits.
        let extras = plain.into_iter().map(|line| line.text).collect();
        LrcDocument {
            synced: true,
            lines: timed,
            metadata,
            extras,
        }
    }
}

/// Write metadata and lines back to LRC. Legacy end tags are preserved until the Studio
/// migrates them. `[offset:]` is not written because it was applied while parsing.
pub fn to_lrc(metadata: &[String], extras: &[String], lines: &[LyricLine]) -> String {
    let mut output = String::new();
    for line in metadata {
        if !line.trim().is_empty() {
            output.push_str(line.trim());
            output.push('\n');
        }
    }
    for line in extras {
        if !line.trim().is_empty() {
            output.push_str(line.trim());
            output.push('\n');
        }
    }
    if lines.is_empty() {
        return output;
    }

    for line in lines {
        if let Some(time) = line.t {
            output.push('[');
            output.push_str(&format_timestamp(time));
            output.push(']');
            if !line.text.is_empty() {
                output.push(' ');
                output.push_str(&line.text);
            }
            if let Some(end) = line.end {
                output.push_str(" [end:");
                output.push_str(&format_timestamp(end));
                output.push(']');
            }
        } else {
            output.push_str(&line.text);
        }
        output.push('\n');
    }
    output
}

/// `mm:ss.xx`, rounded to centiseconds for a readable file.
pub fn format_timestamp(seconds: f64) -> String {
    let centiseconds = (seconds.max(0.0) * 100.0).round() as u64;
    let minutes = centiseconds / 6000;
    let remainder = centiseconds % 6000;
    format!("{minutes:02}:{:05.2}", remainder as f64 / 100.0)
}

/// Parse the minute/second parts used by LRC tags. The seconds part may contain `.` or `:`
/// before a fractional value.
pub fn parse_timestamp(minutes: &str, seconds: &str) -> Option<f64> {
    let minutes: u64 = minutes.trim().parse().ok()?;
    let separator = seconds.find(['.', ':']);
    let (whole, fraction) = match separator {
        Some(index) => (&seconds[..index], &seconds[index + 1..]),
        None => (seconds, ""),
    };
    let whole: u64 = whole.trim().parse().ok()?;
    let fraction = if fraction.is_empty() {
        0.0
    } else {
        let digits: String = fraction.chars().take(3).collect();
        let value: f64 = digits.parse().ok()?;
        value / 10_f64.powi(digits.len() as i32)
    };
    Some(minutes as f64 * 60.0 + whole as f64 + fraction)
}

pub fn is_music_marker(text: &str) -> bool {
    text.trim() == "♪"
}

fn round_millis(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synced_lines_and_metadata_offset() {
        let document = parse_lrc(
            "[ar:Artist]\n[ti:Title]\n[offset:+500]\n[00:01.00]First\n[00:02.50]Second\n",
        );

        assert!(document.synced);
        assert_eq!(document.lines.len(), 2);
        assert_eq!(document.lines[0].t, Some(1.5));
        assert_eq!(document.lines[0].text, "First");
        assert_eq!(document.lines[1].t, Some(3.0));
        assert_eq!(document.metadata, vec!["[ar:Artist]", "[ti:Title]"]);
    }

    #[test]
    fn applies_offset_to_the_whole_file_regardless_of_tag_position() {
        let document = parse_lrc("[00:01.00]Before\n[offset:+500]\n[00:02.00]After\n");

        assert_eq!(document.lines[0].t, Some(1.5));
        assert_eq!(document.lines[1].t, Some(2.5));
    }

    #[test]
    fn clamps_negative_times_after_offset() {
        let document = parse_lrc("[00:00.50]Early\n[offset:-2000]\n");

        assert_eq!(document.lines[0].t, Some(0.0));
    }

    #[test]
    fn parses_multiple_timestamps_on_one_line() {
        let document = parse_lrc("[00:01.00][00:03.00]Again\n");

        assert_eq!(document.lines.len(), 2);
        assert_eq!(document.lines[0].t, Some(1.0));
        assert_eq!(document.lines[1].t, Some(3.0));
        assert_eq!(document.lines[0].text, "Again");
    }

    #[test]
    fn parses_plain_lyrics() {
        let document = parse_lrc("First line\n\nSecond line\n");

        assert!(!document.synced);
        assert_eq!(document.lines.len(), 2);
        assert_eq!(document.lines[0].t, None);
        assert_eq!(document.lines[0].text, "First line");
        assert_eq!(document.lines[1].text, "Second line");
    }

    #[test]
    fn keeps_legacy_end_tags_separate_from_the_text() {
        let document = parse_lrc("[00:01.00]First [end:00:04.25]\n");

        assert_eq!(document.lines[0].text, "First");
        assert_eq!(document.lines[0].end, Some(4.25));
        assert!(document.has_ends());
    }

    #[test]
    fn strips_enhanced_lrc_word_tags() {
        let document = parse_lrc("[00:01.00]<00:01.00>Hello <00:01.50>world\n");

        assert_eq!(document.lines[0].text, "Hello world");
    }

    #[test]
    fn handles_bom_and_crlf() {
        let document = parse_lrc("\u{feff}[00:01.00]Line\r\n");

        assert_eq!(document.lines[0].t, Some(1.0));
        assert_eq!(document.lines[0].text, "Line");
    }

    #[test]
    fn parses_fractional_timestamps_with_two_or_three_digits() {
        assert_eq!(parse_timestamp("01", "02.5"), Some(62.5));
        assert_eq!(parse_timestamp("01", "02.50"), Some(62.5));
        assert_eq!(parse_timestamp("01", "02.500"), Some(62.5));
        assert_eq!(parse_timestamp("01", "02:50"), Some(62.5));
    }

    #[test]
    fn formats_timestamps_for_the_file() {
        assert_eq!(format_timestamp(0.0), "00:00.00");
        assert_eq!(format_timestamp(62.5), "01:02.50");
        assert_eq!(format_timestamp(3599.999), "60:00.00");
    }

    #[test]
    fn round_trips_synced_lines() {
        let document = parse_lrc("[00:01.00]First\n[00:02.50]Second [end:00:03.00]\n");
        let text = document.to_lrc();
        let reparsed = parse_lrc(&text);

        assert_eq!(reparsed.lines, document.lines);
    }

    #[test]
    fn writes_plain_text_without_timestamps() {
        let document = LrcDocument {
            synced: false,
            lines: vec![LyricLine::plain("First"), LyricLine::plain("Second")],
            metadata: Vec::new(),
            extras: Vec::new(),
        };

        assert_eq!(document.to_lrc(), "First\nSecond\n");
    }

    #[test]
    fn keeps_untimed_lines_in_a_mixed_document() {
        let document = parse_lrc("Intro text\n[00:01.00]First\n");
        assert!(document.synced);
        assert_eq!(document.lines.len(), 1);
        assert_eq!(document.extras, vec!["Intro text"]);

        let text = document.to_lrc();
        assert!(text.starts_with("Intro text\n"));
        assert_eq!(parse_lrc(&text).extras, vec!["Intro text"]);
    }

    #[test]
    fn keeps_metadata_when_writing_and_does_not_double_apply_offset() {
        let document = parse_lrc("[ar:Artist]\n[offset:+500]\n[00:01.00]First\n");
        let text = document.to_lrc();

        assert!(text.contains("[ar:Artist]"));
        assert!(!text.contains("[offset:"));

        let reparsed = parse_lrc(&text);
        assert_eq!(reparsed.lines[0].t, Some(1.5));
        assert_eq!(reparsed.metadata, vec!["[ar:Artist]"]);
    }

    #[test]
    fn recognises_music_markers() {
        assert!(is_music_marker("♪"));
        assert!(is_music_marker("  ♪  "));
        assert!(!is_music_marker("not music"));
    }
}
