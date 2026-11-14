use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nico_karaoke_server::updates::{UpdateError, UpdateService, UpdateStatus};
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

/// Updater bridge for the desktop shell. The local server exposes the status; all network and
/// signing work happens here, outside the webview.
pub struct DesktopUpdates {
    app: AppHandle,
    status: Arc<Mutex<UpdateStatus>>,
    pending: Arc<Mutex<Option<Update>>>,
    busy: Arc<AtomicBool>,
}

impl DesktopUpdates {
    pub fn new(app: AppHandle) -> Arc<Self> {
        Arc::new(Self {
            app,
            status: Arc::new(Mutex::new(UpdateStatus::idle())),
            pending: Arc::new(Mutex::new(None)),
            busy: Arc::new(AtomicBool::new(false)),
        })
    }

    fn set_status(&self, status: UpdateStatus) {
        *self
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = status;
    }
}

impl UpdateService for DesktopUpdates {
    fn status(&self) -> UpdateStatus {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn check(&self) -> Result<(), UpdateError> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(UpdateError::Busy);
        }
        self.set_status(UpdateStatus::checking());

        let app = self.app.clone();
        let status = self.status.clone();
        let pending = self.pending.clone();
        let busy = self.busy.clone();
        tauri::async_runtime::spawn(async move {
            let result = match app.updater() {
                Ok(updater) => updater.check().await,
                Err(error) => Err(error),
            };
            match result {
                Ok(Some(update)) => {
                    let version = update.version.clone();
                    let notes = update.body.clone();
                    *pending
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(update);
                    *status
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        UpdateStatus::available(version, notes);
                }
                Ok(None) => {
                    *pending
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
                    *status
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        UpdateStatus::up_to_date();
                }
                Err(error) => {
                    *pending
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
                    *status
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        UpdateStatus::failed(None, error.to_string());
                    tracing::warn!("update check failed");
                    tracing::debug!(error = %error, "update check error");
                }
            }
            busy.store(false, Ordering::SeqCst);
        });
        Ok(())
    }

    fn install(&self) -> Result<(), UpdateError> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(UpdateError::Busy);
        }
        let update = match self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            Some(update) => update,
            None => {
                self.busy.store(false, Ordering::SeqCst);
                return Err(UpdateError::NoUpdate);
            }
        };

        let version = update.version.clone();
        self.set_status(UpdateStatus::downloading(version.clone(), Some(0.0)));
        let app = self.app.clone();
        let status = self.status.clone();
        let pending = self.pending.clone();
        let busy = self.busy.clone();
        tauri::async_runtime::spawn(async move {
            let retry = update.clone();
            let progress_status = status.clone();
            let progress_version = version.clone();
            let mut downloaded: u64 = 0;
            let result = update
                .download_and_install(
                    move |chunk, total| {
                        downloaded = downloaded.saturating_add(chunk as u64);
                        let progress = total
                            .filter(|total| *total > 0)
                            .map(|total| (downloaded as f64 / total as f64).clamp(0.0, 1.0));
                        *progress_status
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                            UpdateStatus::downloading(progress_version.clone(), progress);
                    },
                    || {},
                )
                .await;
            match result {
                Ok(()) => {
                    *status
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        UpdateStatus::installing(version);
                    tracing::info!("update installed; restarting");
                    // The Windows installer exits the app itself; Linux needs the restart.
                    app.request_restart();
                }
                Err(error) => {
                    *pending
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(retry);
                    *status
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        UpdateStatus::failed(Some(version), error.to_string());
                    tracing::warn!("update install failed");
                    tracing::debug!(error = %error, "update install error");
                    busy.store(false, Ordering::SeqCst);
                }
            }
        });
        Ok(())
    }
}
