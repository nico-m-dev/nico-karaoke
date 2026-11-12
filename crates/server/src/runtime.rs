//! Shared startup for the browser server and the desktop shell.

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use nico_karaoke_core::{AppConfig, Library, LyricsLookup};
use tokio::net::TcpListener;

use crate::lrclib_http::HttpLyricsClient;
use crate::watcher::Watcher;
use crate::{AppState, app};

pub const WEB_DIR_ENV: &str = "NICO_KARAOKE_WEB";
const PORT_ATTEMPTS: u16 = 20;

pub struct PreparedServer {
    listener: TcpListener,
    state: AppState,
    address: SocketAddr,
}

impl PreparedServer {
    pub fn url(&self) -> String {
        format!("http://{}/", self.address)
    }

    pub async fn serve(self) -> Result<()> {
        axum::serve(self.listener, app(self.state))
            .await
            .context("server stopped with an error")
    }
}

/// Frontend directory from the environment, a packaged resource, the executable's
/// neighbourhood, or the repository-relative browser build.
pub fn web_dir() -> PathBuf {
    web_dir_with(None)
}

/// Resolve the frontend directory. `resource` lets the desktop shell pass Tauri's resource
/// directory without the server crate depending on Tauri.
pub fn web_dir_with(resource: Option<PathBuf>) -> PathBuf {
    if let Some(value) = std::env::var_os(WEB_DIR_ENV) {
        return PathBuf::from(value);
    }

    let mut candidates = Vec::new();
    if let Some(resource) = resource {
        candidates.push(resource);
    }
    candidates.push(PathBuf::from("web/dist"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(directory) = exe.parent() {
            // A package can keep the assets next to the binary, under /usr/share, or under
            // /usr/lib; all three are checked so the same binary works from any working
            // directory.
            candidates.push(directory.join("web"));
            candidates.push(directory.join("../share/nico-karaoke/web"));
            candidates.push(directory.join("../lib/nico-karaoke/web"));
        }
    }

    first_ready(candidates).unwrap_or_else(|| PathBuf::from("web/dist"))
}

fn first_ready(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| web_dir_is_ready(path))
}

pub fn web_dir_is_ready(web_dir: &Path) -> bool {
    web_dir.join("index.html").is_file() && web_dir.join("editor.html").is_file()
}

/// Bind the socket and build the library, lookup, media provider, and watcher. The caller
/// serves the result on the Tauri runtime or on the browser binary's Tokio runtime.
pub async fn prepare(config: AppConfig, web_dir: PathBuf) -> Result<PreparedServer> {
    let (listener, port) = bind_listener(config.host, config.port)
        .await
        .context("failed to bind the local server")?;
    let address = listener
        .local_addr()
        .context("failed to read the bound address")?;

    let library =
        Arc::new(Library::open(config.library_dir()).context("failed to open the lyrics library")?);
    for warning in library.warnings() {
        tracing::warn!(warning = %warning, "library warning");
    }

    let lyrics_client =
        Arc::new(HttpLyricsClient::new().context("failed to create the LRCLIB client")?);
    let lookup = Arc::new(LyricsLookup::new(lyrics_client));
    let provider = nico_karaoke_platform::create_provider().await;
    let watcher = Arc::new(Watcher::new(provider, lookup, library));
    watcher.start();

    Ok(PreparedServer {
        listener,
        state: AppState {
            config: Arc::new(AppConfig { port, ..config }),
            web_dir: Arc::new(web_dir),
            watcher,
        },
        address,
    })
}

async fn bind_listener(host: IpAddr, port: u16) -> Result<(TcpListener, u16)> {
    for offset in 0..PORT_ATTEMPTS {
        let candidate = port.saturating_add(offset);
        let address = SocketAddr::new(host, candidate);
        match TcpListener::bind(address).await {
            Ok(listener) => {
                let bound_port = listener
                    .local_addr()
                    .context("failed to read the bound address")?
                    .port();
                return Ok((listener, bound_port));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("cannot bind {address}"));
            }
        }
    }

    anyhow::bail!(
        "no free port in the range {port}..{}",
        port.saturating_add(PORT_ATTEMPTS)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_first_ready_frontend_directory() {
        let temp = tempfile::tempdir().expect("temp directory");
        let empty = temp.path().join("empty");
        let web = temp.path().join("web");
        std::fs::create_dir(&empty).expect("empty directory");
        std::fs::create_dir(&web).expect("web directory");
        std::fs::write(web.join("index.html"), "index").expect("index");
        std::fs::write(web.join("editor.html"), "editor").expect("editor");

        assert_eq!(first_ready(vec![empty, web.clone()]), Some(web.clone()));
        assert_eq!(first_ready(vec![web.clone()]), Some(web));
        assert_eq!(first_ready(Vec::<PathBuf>::new()), None);
    }
}
