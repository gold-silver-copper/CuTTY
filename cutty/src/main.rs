mod app;
mod events;
mod input;
mod parser;
mod pty;
mod renderer;
mod selection;
mod terminal;
mod text;

fn main() -> anyhow::Result<()> {
    app::run()
}
