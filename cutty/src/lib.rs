#![doc = include_str!("../README.md")]

pub mod parser;
pub mod selection;
pub mod terminal;
pub mod text;

mod app;
mod events;
mod input;
mod pty;
mod renderer;

/// Launches the bundled desktop terminal application.
pub fn run_terminal() -> anyhow::Result<()> {
    app::run()
}
