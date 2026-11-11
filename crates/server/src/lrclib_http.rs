use std::time::Duration;

use async_trait::async_trait;
use nico_karaoke_core::{APP_VERSION, LookupQuery, LrclibTrack, LyricsClient, LyricsError};

pub const LRCLIB_BASE_URL: &str = "https://lrclib.net/api";
const MAX_RESPONSE_BYTES: usize = 2_000_000;

/// Real LRCLIB HTTP client. It lives in the server crate so the domain crate stays free of
/// network/TLS dependencies.
pub struct HttpLyricsClient {
    base: String,
    http: reqwest::Client,
}

impl HttpLyricsClient {
    pub fn new() -> Result<Self, LyricsError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(format!("nico-karaoke/{APP_VERSION} (local, personal use)"))
            .build()
            .map_err(|error| LyricsError::Request(error.without_url().to_string()))?;
        Ok(Self::with_client(LRCLIB_BASE_URL, http))
    }

    pub fn with_client(base: impl Into<String>, http: reqwest::Client) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            http,
        }
    }

    async fn request(
        &self,
        endpoint: &str,
        query: &LookupQuery,
    ) -> Result<(reqwest::StatusCode, String), LyricsError> {
        let response = self
            .http
            .get(format!("{}/{endpoint}", self.base))
            .query(&query.query_pairs())
            .send()
            .await
            .map_err(|error| LyricsError::Request(error.without_url().to_string()))?;

        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(LyricsError::Request(
                "LRCLIB response is too large".to_owned(),
            ));
        }

        let body = read_body(response).await?;
        Ok((status, body))
    }
}

async fn read_body(mut response: reqwest::Response) -> Result<String, LyricsError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| LyricsError::Request(error.without_url().to_string()))?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(LyricsError::Request(
                "LRCLIB response is too large".to_owned(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body).map_err(|error| LyricsError::Decode(error.to_string()))
}

#[async_trait]
impl LyricsClient for HttpLyricsClient {
    async fn get(&self, query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
        let (status, body) = self.request("get", query).await?;
        parse_get_json(status, &body)
    }

    async fn search(&self, query: &LookupQuery) -> Result<Vec<LrclibTrack>, LyricsError> {
        let (status, body) = self.request("search", query).await?;
        parse_search_json(status, &body)
    }
}

pub(crate) fn parse_get_json(
    status: reqwest::StatusCode,
    body: &str,
) -> Result<Vec<LrclibTrack>, LyricsError> {
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    if !status.is_success() {
        return Err(LyricsError::Request(format!(
            "LRCLIB get returned HTTP {status}"
        )));
    }
    let track: LrclibTrack =
        serde_json::from_str(body).map_err(|error| LyricsError::Decode(error.to_string()))?;
    Ok(vec![track])
}

pub(crate) fn parse_search_json(
    status: reqwest::StatusCode,
    body: &str,
) -> Result<Vec<LrclibTrack>, LyricsError> {
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    if !status.is_success() {
        return Err(LyricsError::Request(format!(
            "LRCLIB search returned HTTP {status}"
        )));
    }
    serde_json::from_str(body).map_err(|error| LyricsError::Decode(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_get_and_search_payloads() {
        let get_body = r#"{
            "id": 12,
            "trackName": "Song",
            "artistName": "Artist",
            "albumName": "Album",
            "duration": 180.0,
            "plainLyrics": "Plain",
            "syncedLyrics": "[00:01.00]Synced"
        }"#;
        let tracks = parse_get_json(reqwest::StatusCode::OK, get_body).expect("get parses");
        assert_eq!(tracks.len(), 1);
        assert!(tracks[0].has_synced());

        let search_body = r#"[{"id": 1, "trackName": "Song"}]"#;
        let tracks =
            parse_search_json(reqwest::StatusCode::OK, search_body).expect("search parses");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].track_name, "Song");
        assert!(!tracks[0].usable());
    }

    #[test]
    fn treats_404_as_an_empty_result_and_other_errors_as_failures() {
        assert!(
            parse_get_json(reqwest::StatusCode::NOT_FOUND, "")
                .expect("404 is empty")
                .is_empty()
        );
        assert!(
            parse_search_json(reqwest::StatusCode::NOT_FOUND, "")
                .expect("404 is empty")
                .is_empty()
        );
        assert!(matches!(
            parse_get_json(reqwest::StatusCode::INTERNAL_SERVER_ERROR, ""),
            Err(LyricsError::Request(_))
        ));
        assert!(matches!(
            parse_search_json(reqwest::StatusCode::OK, "not json"),
            Err(LyricsError::Decode(_))
        ));
    }
}
