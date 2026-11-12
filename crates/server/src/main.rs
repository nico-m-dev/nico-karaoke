use anyhow::{Context, Result};
use nico_karaoke_core::{APP_NAME, APP_VERSION, AppConfig, ConfigError, LaunchMode, USAGE};
use nico_karaoke_server::logging::init_tracing;
use nico_karaoke_server::runtime;

#[tokio::main]
async fn main() -> Result<()> {
    let config = match AppConfig::from_env() {
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
    let _log_guard = init_tracing(&config.data_dir);

    let web_dir = runtime::web_dir();
    if !runtime::web_dir_is_ready(&web_dir) {
        tracing::warn!(
            "frontend build not found; run `npm run build` in web/ or set NICO_KARAOKE_WEB"
        );
    }

    let server = runtime::prepare(config.clone(), web_dir)
        .await
        .context("failed to start the local server")?;
    let url = server.url();
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

    server.serve().await
}
