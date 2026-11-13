#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::IsTerminal;
use std::sync::Mutex;

use anyhow::anyhow;
use nico_karaoke_core::{APP_NAME, APP_VERSION, AppConfig, ConfigError, LaunchMode};
use nico_karaoke_server::{logging, runtime};
use tauri::webview::NewWindowResponse;
use tauri::window::Color;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tracing_appender::non_blocking::WorkerGuard;

struct LogGuard(#[allow(dead_code)] Mutex<Option<WorkerGuard>>);

const DESKTOP_USAGE: &str = "Usage: nico-karaoke-desktop [options]\n\nOptions:\n  --host <ip>     Loopback address to bind (default 127.0.0.1; localhost is accepted)\n  --port <port>   First port to try (default 8770)\n  -h, --help      Show usage\n  -V, --version   Show the version";

/// Show a startup error through GTK, which is already the window toolkit on Linux. Returns
/// false when GTK cannot initialise; stderr already carries the message.
#[cfg(target_os = "linux")]
fn show_gtk_error(message: &str) -> bool {
    use gtk::prelude::*;

    if gtk::init().is_err() {
        return false;
    }
    let dialog = gtk::MessageDialog::new(
        None::<&gtk::Window>,
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Error,
        gtk::ButtonsType::Close,
        message,
    );
    dialog.set_title(APP_NAME);
    dialog.run();
    dialog.close();
    true
}

/// Report a startup failure even when the app was launched without a terminal. On Linux this
/// uses a GTK dialog; on Windows the release build has no console at all.
fn report_fatal(message: &str) {
    eprintln!("{message}");

    // A terminal already shows the message; a launcher start has no terminal, so use a dialog.
    if std::io::stderr().is_terminal() {
        return;
    }

    #[cfg(target_os = "linux")]
    let _ = show_gtk_error(message);

    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        use windows::core::HSTRING;

        let text = HSTRING::from(message);
        let title = HSTRING::from(APP_NAME);
        unsafe {
            MessageBoxW(None, &text, &title, MB_OK | MB_ICONERROR);
        }
    }
}

/// The shell only loads a local page. The override exists for Vite HMR, not remote content.
fn open_external(url: &tauri::Url) {
    if !matches!(url.scheme(), "http" | "https") {
        return;
    }

    let url = url.to_string();
    std::thread::spawn(move || {
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
        match command.arg(&url).status() {
            Ok(status) if !status.success() => {
                tracing::warn!(%status, "external opener failed");
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(error = %error, "could not open the external link"),
        }
    });
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

/// Start the server on its own thread and return its bound URL.
///
/// The Windows SMTC provider initializes COM as MTA. Doing that on the Tauri main thread makes
/// WebView2 window creation fail afterwards. The Python predecessor also kept its server on a
/// separate thread, which kept the UI thread's COM apartment clean.
fn start_server(config: AppConfig, web_dir: std::path::PathBuf) -> Result<String, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("nico-karaoke-server".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = tx.send(Err(format!("could not create the server runtime: {error}")));
                    return;
                }
            };
            let prepared = match runtime.block_on(runtime::prepare(config, web_dir)) {
                Ok(prepared) => prepared,
                Err(error) => {
                    let _ = tx.send(Err(format!("{error:#}")));
                    return;
                }
            };
            let url = prepared.url();
            if tx.send(Ok(url)).is_err() {
                return;
            }
            if let Err(error) = runtime.block_on(prepared.serve()) {
                tracing::error!(error = %error, "local server stopped");
            }
        })
        .map_err(|error| format!("could not start the server thread: {error}"))?;

    rx.recv_timeout(std::time::Duration::from_secs(30))
        .map_err(|_| "the local server did not start within 30 seconds".to_owned())?
}

fn main() {
    let config = match AppConfig::from_env() {
        Ok(config) => config,
        Err(ConfigError::Help) => {
            println!("{DESKTOP_USAGE}");
            return;
        }
        Err(ConfigError::Version) => {
            println!("{APP_NAME} {APP_VERSION}");
            return;
        }
        Err(error) => {
            report_fatal(&format!("failed to read configuration: {error}"));
            std::process::exit(2);
        }
    };

    // The webview is a local page by design. Reject a public bind before opening the listener
    // instead of panicking later when the window URL is built.
    if !config.host.is_loopback() {
        report_fatal(
            "the desktop window only supports a loopback host (127.0.0.1 or ::1); use the server binary for other hosts",
        );
        std::process::exit(2);
    }
    if config.launch != LaunchMode::Auto {
        report_fatal("--browser and --no-open are only supported by nico-karaoke-server");
        std::process::exit(2);
    }

    let webview_override = std::env::var("NICO_KARAOKE_WEBVIEW_URL")
        .ok()
        .filter(|value| !value.is_empty());

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(
            |app, _arguments, _cwd| {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            },
        ))
        .setup(move |app| {
            let guard = logging::init_tracing(&config.data_dir);
            app.manage(LogGuard(Mutex::new(guard)));

            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                tracing::error!("panic: {info}");
                report_fatal(&format!("Nico Karaoke crashed: {info}"));
                default_hook(info);
            }));

            let web_dir =
                runtime::web_dir_with(app.path().resource_dir().ok().map(|dir| dir.join("web")));
            // HMR serves the page from Vite, so a local build output is not required then.
            if webview_override.is_none()
                && !runtime::web_dir_is_ready(&web_dir)
                && !nico_karaoke_server::has_embedded_assets()
            {
                let message =
                    "frontend build not found; run `npm run build` in web/ or set NICO_KARAOKE_WEB";
                tracing::error!("{message}");
                report_fatal(message);
                std::process::exit(2);
            }

            let server_url = match start_server(config.clone(), web_dir) {
                Ok(url) => url,
                Err(error) => {
                    let message = format!("failed to start the local server: {error}");
                    tracing::error!("{message}");
                    report_fatal(&message);
                    std::process::exit(2);
                }
            };
            tracing::info!(
                data = %config.data_dir.display(),
                url = %server_url,
                "starting desktop shell"
            );

            let raw_url = webview_override.unwrap_or(server_url);
            let url = match local_view_url(&raw_url) {
                Ok(url) => url,
                Err(error) => {
                    report_fatal(&format!("invalid webview URL: {error}"));
                    std::process::exit(2);
                }
            };
            tracing::info!(url = %url, "opening window");
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title(APP_NAME)
                .inner_size(1280.0, 800.0)
                .min_inner_size(360.0, 300.0)
                .background_color(Color(8, 11, 10, 255))
                .on_navigation(|url| local_view_url(url.as_str()).is_ok())
                .on_new_window(|url, _features| {
                    open_external(&url);
                    NewWindowResponse::Deny
                })
                .build();
            if let Err(error) = window {
                report_fatal(&format!("could not open the window: {error}"));
                std::process::exit(2);
            }
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(error) = result {
        report_fatal(&format!("Nico Karaoke could not start: {error}"));
        std::process::exit(1);
    }
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
