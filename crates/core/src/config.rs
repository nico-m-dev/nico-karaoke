use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use directories::ProjectDirs;
use thiserror::Error;

pub const APP_NAME: &str = "Nico Karaoke";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_PORT: u16 = 8770;
pub const DATA_DIR_ENV: &str = "NICO_KARAOKE_DATA";

/// How the app should open after the local server starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// Open a native window when available, otherwise the browser.
    Auto,
    /// Always use the default browser.
    Browser,
    /// Do not open anything.
    NoOpen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub data_dir: PathBuf,
    pub launch: LaunchMode,
}

impl AppConfig {
    /// Read configuration from process arguments and the environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        let data_dir = env::var_os(DATA_DIR_ENV)
            .map(PathBuf::from)
            .unwrap_or(default_data_dir()?);
        Self::from_args(env::args().skip(1), data_dir)
    }

    /// Parse arguments against explicit defaults. Keeping the data directory out of the
    /// parser makes this testable without mutating the process environment.
    pub fn from_args<I>(args: I, data_dir: PathBuf) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut host = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let mut port = DEFAULT_PORT;
        let mut launch = LaunchMode::Auto;
        let mut args = args.into_iter();

        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--host" => {
                    let value = args.next().ok_or(ConfigError::MissingValue("--host"))?;
                    host = value.parse().map_err(|_| ConfigError::InvalidHost(value))?;
                }
                "--port" => {
                    let value = args.next().ok_or(ConfigError::MissingValue("--port"))?;
                    port = value.parse().map_err(|_| ConfigError::InvalidPort(value))?;
                }
                "--browser" => launch = LaunchMode::Browser,
                "--no-open" => launch = LaunchMode::NoOpen,
                other => return Err(ConfigError::UnknownArgument(other.to_owned())),
            }
        }

        Ok(Self {
            host,
            port,
            data_dir,
            launch,
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

/// Platform-specific local data directory, resolved by the `directories` crate.
pub fn default_data_dir() -> Result<PathBuf, ConfigError> {
    ProjectDirs::from("dev", "nico", "nico-karaoke")
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .ok_or(ConfigError::NoDataDir)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing value for {0}")]
    MissingValue(&'static str),
    #[error("invalid host: {0}")]
    InvalidHost(String),
    #[error("invalid port: {0}")]
    InvalidPort(String),
    #[error("unknown argument: {0}")]
    UnknownArgument(String),
    #[error("could not determine a local data directory")]
    NoDataDir,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn defaults_to_localhost_and_default_port() {
        let config = AppConfig::from_args(args(&[]), PathBuf::from("test-data"))
            .expect("defaults should parse");

        assert_eq!(config.host, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(config.launch, LaunchMode::Auto);
        assert_eq!(config.bind_addr().to_string(), "127.0.0.1:8770");
    }

    #[test]
    fn parses_host_and_port() {
        let config = AppConfig::from_args(
            args(&["--host", "0.0.0.0", "--port", "9000"]),
            PathBuf::from("test-data"),
        )
        .expect("valid arguments should parse");

        assert_eq!(config.host, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(config.port, 9000);
    }

    #[test]
    fn parses_launch_modes() {
        let browser = AppConfig::from_args(args(&["--browser"]), PathBuf::from("test-data"))
            .expect("browser mode should parse");
        assert_eq!(browser.launch, LaunchMode::Browser);

        let no_open = AppConfig::from_args(args(&["--no-open"]), PathBuf::from("test-data"))
            .expect("no-open mode should parse");
        assert_eq!(no_open.launch, LaunchMode::NoOpen);
    }

    #[test]
    fn rejects_missing_values() {
        let error = AppConfig::from_args(args(&["--port"]), PathBuf::from("test-data"))
            .expect_err("missing port should fail");
        assert_eq!(error, ConfigError::MissingValue("--port"));
    }

    #[test]
    fn rejects_invalid_port() {
        let error = AppConfig::from_args(args(&["--port", "abc"]), PathBuf::from("test-data"))
            .expect_err("invalid port should fail");
        assert_eq!(error, ConfigError::InvalidPort("abc".to_owned()));
    }

    #[test]
    fn rejects_unknown_arguments() {
        let error = AppConfig::from_args(args(&["--wat"]), PathBuf::from("test-data"))
            .expect_err("unknown argument should fail");
        assert_eq!(error, ConfigError::UnknownArgument("--wat".to_owned()));
    }
}
