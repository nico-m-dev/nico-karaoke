//! Platform boundary for Nico Karaoke.
//!
//! The core crate knows only the `MediaProvider` trait. This crate supplies the Linux MPRIS
//! provider and the Windows SMTC provider behind that trait.

use std::sync::Arc;

use async_trait::async_trait;
use nico_karaoke_core::{MediaError, MediaProvider, PlaybackAction, PlayerSnapshot};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

pub const SUPPORTED_OPERATING_SYSTEMS: [&str; 2] = ["linux", "windows"];

pub fn operating_system() -> &'static str {
    std::env::consts::OS
}

pub fn architecture() -> &'static str {
    std::env::consts::ARCH
}

pub fn is_supported() -> bool {
    SUPPORTED_OPERATING_SYSTEMS.contains(&operating_system())
}

pub fn target_label() -> String {
    format!("{}/{}", operating_system(), architecture())
}

/// Build the media provider for this platform. A backend that cannot start is replaced by a
/// provider that reports the reason instead of taking the app down.
pub async fn create_provider() -> Arc<dyn MediaProvider> {
    #[cfg(target_os = "linux")]
    {
        match linux::LinuxMediaProvider::new().await {
            Ok(provider) => Arc::new(provider),
            Err(MediaError::Unavailable(reason)) => Arc::new(UnavailableMediaProvider::new(reason)),
            Err(error) => Arc::new(UnavailableMediaProvider::new(error.to_string())),
        }
    }
    #[cfg(target_os = "windows")]
    {
        Arc::new(windows::SmtcMediaProvider::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Arc::new(UnavailableMediaProvider::new(format!(
            "media providers are not implemented for {}",
            operating_system()
        )))
    }
}

/// Fallback provider used when the platform backend is unavailable.
pub struct UnavailableMediaProvider {
    reason: String,
}

impl UnavailableMediaProvider {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl MediaProvider for UnavailableMediaProvider {
    async fn snapshots(&self) -> Result<Vec<PlayerSnapshot>, MediaError> {
        Err(MediaError::Unavailable(self.reason.clone()))
    }

    async fn control(&self, _player: &str, _action: PlaybackAction) -> Result<(), MediaError> {
        Err(MediaError::Unavailable(self.reason.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_non_empty_target() {
        assert!(!target_label().is_empty());
        assert!(!operating_system().is_empty());
        assert!(!architecture().is_empty());
    }

    #[test]
    fn supported_list_contains_only_known_targets() {
        for target in SUPPORTED_OPERATING_SYSTEMS {
            assert!(matches!(target, "linux" | "windows"));
        }
    }

    #[tokio::test]
    async fn unavailable_provider_reports_its_reason() {
        let provider = UnavailableMediaProvider::new("no backend");
        let error = provider
            .snapshots()
            .await
            .expect_err("provider is unavailable");
        assert!(matches!(error, MediaError::Unavailable(reason) if reason == "no backend"));
    }
}
