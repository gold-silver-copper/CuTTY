#![doc = include_str!("../README.md")]

pub mod config;
pub mod parser;
pub mod selection;
pub mod terminal;
pub mod text;

mod app;
mod events;
mod input;
mod pty;
mod renderer;

pub use config::AppConfig;

/// Launches the bundled desktop terminal application.
pub fn run_terminal() -> anyhow::Result<()> {
    run_terminal_with_config(AppConfig::default())
}

/// Launches the bundled desktop terminal application with an explicit config.
pub fn run_terminal_with_config(config: AppConfig) -> anyhow::Result<()> {
    config.validate()?;
    app::run(config)
}
