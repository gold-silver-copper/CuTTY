<h1 align="center">CuTTY, the Copper Teletype</h1>

<p align="center">
  <img alt="CuTTY, the Copper Teletype"
       src="./CuTTY.png">
</p>

## About

CuTTY is a modern terminal emulator that comes with sensible defaults, but
allows for extensive [configuration](#configuration). By integrating with other
applications, rather than reimplementing their functionality, it manages to
provide a flexible set of [features](./docs/features.md) with high performance.
The supported platforms currently consist of BSD, Linux, macOS and Windows.

CuTTY is a fork of Alacritty that replaces the old OpenGL renderer with a
`wgpu`/`vello` rendering stack and a `parley` text pipeline. This allows CuTTY
to target modern graphics backends like Vulkan, Metal, and DirectX 12 instead
of depending on an OpenGL rendering path. Replacing the hand-rolled OpenGL
renderer with the Vello/Parley stack also cut roughly 4,000 lines of renderer
and font-handling code. The terminal core and PTY behavior remain split into
the `cutty_terminal` crate.

The software is considered to be at a **beta** level of readiness; there are
a few missing features and bugs to be fixed, but it is already used by many as
a daily driver.

## Features

You can find an overview over the features available in CuTTY [here](./docs/features.md).

## Installation

CuTTY can be installed directly in a few simple ways:

- Windows: download a release binary from the GitHub Releases page.
- macOS: either download a release binary from the GitHub Releases page, or install it the Unix way.

- Linux/BSD/Unix: install with:

  ```sh
  cargo install cutty
  ```

Build and installation instructions live in [INSTALL.md](./INSTALL.md).

### Requirements

- A graphics adapter and driver supported by `wgpu`
- [Windows] ConPTY support (Windows 10 version 1809 or higher)

## Configuration

You can find the documentation for CuTTY's configuration in `man 5 cutty`, or
in the source manpage at [extra/man/cutty.5.scd](./extra/man/cutty.5.scd).
There is also a full example configuration at
[extra/cutty.example.toml](./extra/cutty.example.toml), and a practical
day-to-day starter config at
[extra/cutty.daily.toml](./extra/cutty.daily.toml).

CuTTY doesn't create the config file for you, but it looks for one in the
following locations:

1. `$XDG_CONFIG_HOME/cutty/cutty.toml`
2. `$XDG_CONFIG_HOME/cutty.toml`
3. `$HOME/.config/cutty/cutty.toml`
4. `$HOME/.cutty.toml`
5. `/etc/cutty/cutty.toml`

On Windows, the config file will be looked for in:

* `%APPDATA%\cutty\cutty.toml`

CuTTY only supports TOML configuration files. Legacy YAML configs are not
supported.

If you're coming from Alacritty, the main difference is fonts: CuTTY's
`[font.*].family` values can be CSS-style fallback lists like
`"FiraMono Nerd Font, ui-monospace, monospace"` instead of a single family
name.

## Contributing

A guideline about contributing to CuTTY can be found in the
[`CONTRIBUTING.md`](CONTRIBUTING.md) file.

## FAQ

**_Is it really the fastest terminal emulator?_**

Benchmarking terminal emulators is complicated. CuTTY uses
[vtebench](https://github.com/alacritty/vtebench) and kitty's performance
benchmark to quantify terminal throughput. The repo includes a single runner,
[`bench.sh`](./bench.sh), which launches CuTTY, Alacritty, Kitty, and Ghostty
sequentially, records both benchmark suites, and prints a final winner report.

On the local runs from March 18, 2026, CuTTY won 4 of 5 kitty benchmark
categories and all 10 measured `vtebench` categories.

Kitty benchmark results from March 18, 2026:

| Test | CuTTY | Alacritty | Kitty | Ghostty | Winner |
| --- | --- | --- | --- | --- | --- |
| CSI codes with few chars | `1.38s @ 72.4 MB/s` | `1.76s @ 56.9 MB/s` | `1.74s @ 57.5 MB/s` | `2.36s @ 42.3 MB/s` | CuTTY |
| Images | `1.56s @ 342.1 MB/s` | `2.29s @ 233.0 MB/s` | `2.04s @ 261.4 MB/s` | `10.35s @ 51.5 MB/s` | CuTTY |
| Long escape codes | `4.82s @ 162.7 MB/s` | `6.28s @ 124.9 MB/s` | `2.42s @ 324.1 MB/s` | `11.02s @ 71.2 MB/s` | Kitty |
| Only ASCII chars | `1.88s @ 106.5 MB/s` | `2.57s @ 77.7 MB/s` | `2.07s @ 96.4 MB/s` | `2.58s @ 77.5 MB/s` | CuTTY |
| Unicode chars | `1.28s @ 141.1 MB/s` | `2.04s @ 88.6 MB/s` | `1.46s @ 124.1 MB/s` | `1.73s @ 104.3 MB/s` | CuTTY |

| Terminal | Wins |
| --- | --- |
| CuTTY | 4 |
| Alacritty | 0 |
| Kitty | 1 |
| Ghostty | 0 |

`vtebench` results from March 18, 2026:

| Test | CuTTY | Alacritty | Kitty | Ghostty | Winner |
| --- | --- | --- | --- | --- | --- |
| dense_cells | `8.04ms avg (90% < 8ms)` | `10.23ms avg (90% < 11ms)` | `17.99ms avg (90% < 18ms)` | `9.57ms avg (90% < 10ms)` | CuTTY |
| medium_cells | `11.06ms avg (90% < 12ms)` | `12.11ms avg (90% < 13ms)` | `14.19ms avg (90% < 15ms)` | `13.66ms avg (90% < 14ms)` | CuTTY |
| scrolling | `112.21ms avg (90% < 113ms)` | `120.73ms avg (90% < 122ms)` | `163.10ms avg (90% < 183ms)` | `115.32ms avg (90% < 117ms)` | CuTTY |
| scrolling_bottom_region | `111.58ms avg (90% < 112ms)` | `120.70ms avg (90% < 122ms)` | `115.21ms avg (90% < 116ms)` | `116.15ms avg (90% < 118ms)` | CuTTY |
| scrolling_bottom_small_region | `111.30ms avg (90% < 112ms)` | `120.70ms avg (90% < 122ms)` | `114.79ms avg (90% < 116ms)` | `115.71ms avg (90% < 117ms)` | CuTTY |
| scrolling_fullscreen | `187.54ms avg (90% < 189ms)` | `194.91ms avg (90% < 196ms)` | `302.24ms avg (90% < 311ms)` | `194.41ms avg (90% < 196ms)` | CuTTY |
| scrolling_top_region | `114.48ms avg (90% < 116ms)` | `124.51ms avg (90% < 125ms)` | `115.09ms avg (90% < 116ms)` | `116.45ms avg (90% < 118ms)` | CuTTY |
| scrolling_top_small_region | `111.98ms avg (90% < 113ms)` | `121.12ms avg (90% < 122ms)` | `115.09ms avg (90% < 116ms)` | `116.44ms avg (90% < 118ms)` | CuTTY |
| sync_medium_cells | `12.45ms avg (90% < 13ms)` | `15.11ms avg (90% < 15ms)` | `45.89ms avg (90% < 46ms)` | `16.36ms avg (90% < 17ms)` | CuTTY |
| unicode | `7.65ms avg (90% < 8ms)` | `10.60ms avg (90% < 11ms)` | `14.18ms avg (90% < 9ms)` | `9.03ms avg (90% < 9ms)` | CuTTY |

| Terminal | Wins |
| --- | --- |
| CuTTY | 10 |
| Alacritty | 0 |
| Kitty | 0 |
| Ghostty | 0 |

Those numbers are specific to that benchmark set, machine, and configuration.
The best way to evaluate terminal performance is still to test your own setup.
If you have found an example where this is not the case, please report a bug.

Other aspects like latency or framerate and frame consistency are more difficult
to quantify. Some terminal emulators also intentionally slow down to save
resources, which might be preferred by some users.

If you have doubts about CuTTY's performance or usability, the best way to
quantify terminal emulators is always to test them with **your** specific
usecases.

**_Why isn't feature X implemented?_**

CuTTY has many great features, but not every feature from every other
terminal. This could be for a number of reasons, but sometimes it's just not a
good fit for CuTTY. This means you won't find things like tabs or splits
(which are best left to a window manager or [terminal multiplexer][tmux]) nor
niceties like a GUI config editor.

[tmux]: https://github.com/tmux/tmux

## License

CuTTY is released under the [Apache License, Version 2.0](./LICENSE-APACHE).
