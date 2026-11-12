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
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn url(&self) -> String {
        format!("http://{}/", self.address)
    }

    pub async fn serve(self) -> Result<()> {
        axum::serve(self.listener, app(self.state))
            .await
            .context("server stopped with an error")
    }
}

/// Frontend directory from the environment, or the repository-relative browser build.
pub fn web_dir() -> PathBuf {
    std::env::var_os(WEB_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("web/dist"))
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
