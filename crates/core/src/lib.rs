//! Domain logic for Nico Karaoke.
//!
//! This crate must stay independent of HTTP, desktop-shell, and platform APIs. Media
//! providers and lyrics clients are injected through traits so tests can run without a real
//! player or network.

pub mod config;

pub use config::{APP_NAME, APP_VERSION, AppConfig, ConfigError, DEFAULT_PORT, LaunchMode};
