# cutty

A library-first terminal crate for CuTTY.

`cutty` exposes the terminal state, ANSI parser, selection model, and text
shaping utilities as a reusable Rust library. The desktop terminal application
ships as the companion `cutty` binary target.

Run the terminal locally with:

```bash
cargo run --bin cutty
```

## Configuration

The `cutty` binary reads `config.toml` from the platform config directory:

- macOS: `~/Library/Application Support/cutty/config.toml`
- Linux: `~/.config/cutty/config.toml`

Set `CUTTY_CONFIG=/path/to/config.toml` to override that location.

Example:

```toml
[font]
families = ["JetBrainsMono Nerd Font Mono", "Symbols Nerd Font Mono", "monospace"]
size = 18.0
line_height = 1.25

[window]
width = 1180
height = 760

[terminal]
scrollback = 5000
shell = "/bin/zsh"
```
