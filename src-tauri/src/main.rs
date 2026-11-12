#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use anyhow::{Context, anyhow};
use nico_karaoke_core::{APP_NAME, APP_VERSION, AppConfig, ConfigError, USAGE};
use nico_karaoke_server::{logging, runtime};
use tauri::webview::NewWindowResponse;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tracing_appender::non_blocking::WorkerGuard;

struct LogGuard(#[allow(dead_code)] Mutex<Option<WorkerGuard>>);

/// The shell only loads a local page. The override exists for Vite HMR, not remote content.
fn open_external(url: &tauri::Url) {
    if !matches!(url.scheme(), "http" | "https") {
        return;
    }

    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    command.arg(url.as_str());
    if let Err(error) = command.spawn() {
        tracing::warn!(error = %error, "could not open the external link");
    }
}

fn local_view_url(raw: &str) -> anyhow::Result<tauri::Url> {
    let url = raw.parse::<tauri::Url>()?;
    let local =
        url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    if !local {
        return Err(anyhow!("webview URL must be a local http URL"));
    }
    Ok(url)
}

fn main() {
    let config = match AppConfig::from_env() {
        Ok(config) => config,
        Err(ConfigError::Help) => {
            println!("{USAGE}");
            return;
        }
        Err(ConfigError::Version) => {
            println!("{APP_NAME} {APP_VERSION}");
            return;
        }
        Err(error) => {
            eprintln!("failed to read configuration: {error}");
            std::process::exit(2);
        }
    };

    tauri::Builder::default()
        .setup(move |app| {
            let guard = logging::init_tracing(&config.data_dir);
            app.manage(LogGuard(Mutex::new(guard)));

            let web_dir = runtime::web_dir();
            if !runtime::web_dir_is_ready(&web_dir) {
                tracing::error!(
                    "frontend build not found; run `npm run build` in web/ or set NICO_KARAOKE_WEB"
                );
                return Err(anyhow!(
                    "frontend build not found; run `npm run build` in web/ or set NICO_KARAOKE_WEB"
                )
                .into());
            }

            let prepared =
                match tauri::async_runtime::block_on(runtime::prepare(config.clone(), web_dir)) {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        tracing::error!(
                            error = %format!("{error:#}"),
                            "failed to start the local server"
                        );
                        return Err(error)
                            .context("failed to start the local server")
                            .map_err(Into::into);
                    }
                };
            let server_url = prepared.url();
            tracing::info!(
                data = %config.data_dir.display(),
                url = %server_url,
                "starting desktop shell"
            );
            tauri::async_runtime::spawn(async move {
                if let Err(error) = prepared.serve().await {
                    tracing::error!(error = %error, "local server stopped");
                }
            });

            let raw_url = std::env::var("NICO_KARAOKE_WEBVIEW_URL")
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or(server_url);
            let url = local_view_url(&raw_url)?;
            tracing::info!(url = %url, "opening window");
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("Nico Karaoke")
                .inner_size(1280.0, 800.0)
                .min_inner_size(800.0, 560.0)
                .on_new_window(|url, _features| {
                    open_external(&url);
                    NewWindowResponse::Deny
                })
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Nico Karaoke");
}

#[cfg(test)]
mod tests {
    use super::local_view_url;

    #[test]
    fn accepts_only_local_http_urls() {
        assert!(local_view_url("http://127.0.0.1:8770/").is_ok());
        assert!(local_view_url("http://localhost:5173/editor").is_ok());
        assert!(local_view_url("https://127.0.0.1:8770/").is_err());
        assert!(local_view_url("http://example.com/").is_err());
        assert!(local_view_url("file:///tmp/index.html").is_err());
    }
}
