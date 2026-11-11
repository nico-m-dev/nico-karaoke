use std::env;
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use anyhow::{Context, Result};
use nico_karaoke_core::{APP_NAME, APP_VERSION, AppConfig, ConfigError, LaunchMode, USAGE};
use nico_karaoke_server::{AppState, app};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

const PORT_ATTEMPTS: u16 = 20;
const WEB_DIR_ENV: &str = "NICO_KARAOKE_WEB";

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let mut config = match AppConfig::from_env() {
        Ok(config) => config,
        Err(ConfigError::Help) => {
            println!("{USAGE}");
            return Ok(());
        }
        Err(ConfigError::Version) => {
            println!("{APP_NAME} {APP_VERSION}");
            return Ok(());
        }
        Err(error) => return Err(error).context("failed to read configuration"),
    };
    let (listener, port) = bind_listener(config.host, config.port)
        .await
        .context("failed to bind the local server")?;
    config.port = port;

    let web_dir = env::var_os(WEB_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("web/dist"));
    if !web_dir.join("index.html").is_file() {
        tracing::warn!(
            path = %web_dir.display(),
            "frontend build not found; run `npm run build` in web/ or set NICO_KARAOKE_WEB"
        );
    }

    let url = format!("http://{}/", config.bind_addr());
    tracing::debug!(path = %config.data_dir.display(), "data directory");
    tracing::info!(
        name = APP_NAME,
        version = APP_VERSION,
        url = %url,
        "starting"
    );
    match config.launch {
        LaunchMode::NoOpen => tracing::info!("server-only mode; opening nothing"),
        LaunchMode::Browser | LaunchMode::Auto => {
            tracing::info!("open the URL above; automatic opening is not wired yet")
        }
    }

    let state = AppState {
        config: config.into(),
        web_dir: web_dir.into(),
    };
    axum::serve(listener, app(state))
        .await
        .context("server stopped with an error")
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
            Err(error) if error.kind() == ErrorKind::AddrInUse => continue,
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

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("nico_karaoke_server=info,tower_http=warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
