#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use anyhow::{Context, anyhow};
use nico_karaoke_core::AppConfig;
use nico_karaoke_server::{logging, runtime};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tracing_appender::non_blocking::WorkerGuard;

struct LogGuard(#[allow(dead_code)] Mutex<Option<WorkerGuard>>);

/// The shell only loads a local page. The override exists for Vite HMR, not remote content.
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
    tauri::Builder::default()
        .setup(|app| {
            let config = AppConfig::from_env().map_err(|error| anyhow!(error))?;
            let guard = logging::init_tracing(&config.data_dir);
            app.manage(LogGuard(Mutex::new(guard)));

            let web_dir = runtime::web_dir();
            if !runtime::web_dir_is_ready(&web_dir) {
                return Err(anyhow!(
                    "frontend build not found; run `npm run build` in web/ or set NICO_KARAOKE_WEB"
                )
                .into());
            }

            let prepared =
                tauri::async_runtime::block_on(runtime::prepare(config.clone(), web_dir))
                    .context("failed to start the local server")?;
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

            let raw_url = std::env::var("NICO_KARAOKE_WEBVIEW_URL").unwrap_or(server_url);
            let url = local_view_url(&raw_url)?;
            tracing::info!(url = %url, "opening window");
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("Nico Karaoke")
                .inner_size(1280.0, 800.0)
                .min_inner_size(800.0, 560.0)
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
