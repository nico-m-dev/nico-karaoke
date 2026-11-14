use serde::Serialize;

/// Lifecycle of an in-app update check or install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    Disabled,
    Idle,
    Checking,
    Available,
    UpToDate,
    Downloading,
    Installing,
    Error,
}

/// Snapshot for the settings panel. The desktop shell fills this from the Tauri updater; the
/// standalone server reports `supported: false`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateStatus {
    pub supported: bool,
    pub phase: UpdatePhase,
    pub version: Option<String>,
    pub notes: Option<String>,
    /// Download progress from 0.0 to 1.0 when the content length is known.
    pub progress: Option<f64>,
    pub error: Option<String>,
}

impl UpdateStatus {
    pub fn disabled() -> Self {
        Self {
            supported: false,
            phase: UpdatePhase::Disabled,
            version: None,
            notes: None,
            progress: None,
            error: None,
        }
    }

    pub fn idle() -> Self {
        Self {
            supported: true,
            phase: UpdatePhase::Idle,
            version: None,
            notes: None,
            progress: None,
            error: None,
        }
    }

    pub fn checking() -> Self {
        Self {
            phase: UpdatePhase::Checking,
            ..Self::idle()
        }
    }

    pub fn available(version: impl Into<String>, notes: Option<String>) -> Self {
        Self {
            phase: UpdatePhase::Available,
            version: Some(version.into()),
            notes,
            ..Self::idle()
        }
    }

    pub fn up_to_date() -> Self {
        Self {
            phase: UpdatePhase::UpToDate,
            ..Self::idle()
        }
    }

    pub fn downloading(version: impl Into<String>, progress: Option<f64>) -> Self {
        Self {
            phase: UpdatePhase::Downloading,
            version: Some(version.into()),
            progress,
            ..Self::idle()
        }
    }

    pub fn installing(version: impl Into<String>) -> Self {
        Self {
            phase: UpdatePhase::Installing,
            version: Some(version.into()),
            ..Self::idle()
        }
    }

    pub fn failed(version: Option<String>, message: impl Into<String>) -> Self {
        Self {
            phase: UpdatePhase::Error,
            version,
            error: Some(message.into()),
            ..Self::idle()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError {
    Unsupported,
    Busy,
    NoUpdate,
}

/// The desktop shell injects an implementation backed by the Tauri updater. The server crate
/// only knows this trait so it keeps working without Tauri.
pub trait UpdateService: Send + Sync {
    fn status(&self) -> UpdateStatus;
    fn check(&self) -> Result<(), UpdateError>;
    fn install(&self) -> Result<(), UpdateError>;
}

/// Used by the standalone server and tests: the version is visible, updates are not.
#[derive(Debug, Default)]
pub struct DisabledUpdates;

impl UpdateService for DisabledUpdates {
    fn status(&self) -> UpdateStatus {
        UpdateStatus::disabled()
    }

    fn check(&self) -> Result<(), UpdateError> {
        Err(UpdateError::Unsupported)
    }

    fn install(&self) -> Result<(), UpdateError> {
        Err(UpdateError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_status_is_explicit_and_unsupported() {
        let status = DisabledUpdates.status();
        assert!(!status.supported);
        assert_eq!(status.phase, UpdatePhase::Disabled);
        assert_eq!(DisabledUpdates.check(), Err(UpdateError::Unsupported));
    }

    #[test]
    fn serializes_phases_as_snake_case() {
        let value = serde_json::to_value(UpdateStatus::up_to_date()).expect("json");
        assert_eq!(value["phase"], "up_to_date");
        assert_eq!(value["supported"], true);
    }

    #[test]
    fn a_failed_status_can_keep_the_attempted_version() {
        let status = UpdateStatus::failed(Some("1.2.0".to_owned()), "download failed");
        assert_eq!(status.phase, UpdatePhase::Error);
        assert_eq!(status.version.as_deref(), Some("1.2.0"));
        assert_eq!(status.error.as_deref(), Some("download failed"));
    }
}
