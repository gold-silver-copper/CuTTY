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

CuTTY can be installed by using various package managers on Linux, BSD,
macOS and Windows.

Build and installation instructions live in [INSTALL.md](./INSTALL.md).

### Requirements

- A graphics adapter and driver supported by `wgpu`
- [Windows] ConPTY support (Windows 10 version 1809 or higher)

## Configuration

You can find the documentation for CuTTY's configuration in `man 5 cutty`, or
in the source manpage at [extra/man/cutty.5.scd](./extra/man/cutty.5.scd).

CuTTY doesn't create the config file for you, but it looks for one in the
following locations:

1. `$XDG_CONFIG_HOME/cutty/cutty.toml`
2. `$XDG_CONFIG_HOME/cutty.toml`
3. `$HOME/.config/cutty/cutty.toml`
4. `$HOME/.cutty.toml`
5. `/etc/cutty/cutty.toml`

On Windows, the config file will be looked for in:

* `%APPDATA%\cutty\cutty.toml`

## Contributing

A guideline about contributing to CuTTY can be found in the
[`CONTRIBUTING.md`](CONTRIBUTING.md) file.

## FAQ

**_Is it really the fastest terminal emulator?_**

Benchmarking terminal emulators is complicated. CuTTY uses
[vtebench](https://github.com/alacritty/vtebench) and kitty's performance
benchmark to quantify terminal throughput. On the local runs from March 17,
2026, CuTTY beat Alacritty across the measured benchmark set, including the
long-escape workload after the escape-path optimization work.

Kitty benchmark results from March 17, 2026:

| Test | Alacritty | CuTTY |
| --- | --- | --- |
| Only ASCII chars | `2.61s @ 76.7 MB/s` | `1.78s @ 112.5 MB/s` |
| Unicode chars | `2.04s @ 88.7 MB/s` | `1.28s @ 141.5 MB/s` |
| CSI codes with few chars | `1.78s @ 56.2 MB/s` | `1.37s @ 73.2 MB/s` |
| Long escape codes | `5.11s @ 153.5 MB/s` | `4.27s @ 183.8 MB/s` |
| Images | `2.31s @ 231.3 MB/s` | `1.57s @ 340.5 MB/s` |

`vtebench` results from March 17, 2026:

| Test | Alacritty | CuTTY |
| --- | --- | --- |
| dense_cells | `11.3ms avg (90% < 13ms)` | `8.28ms avg (90% < 9ms)` |
| medium_cells | `12.24ms avg (90% < 14ms)` | `10.03ms avg (90% < 12ms)` |
| scrolling | `20.52ms avg (90% < 23ms)` | `18.83ms avg (90% < 21ms)` |
| scrolling_bottom_region | `18.59ms avg (90% < 21ms)` | `14.57ms avg (90% < 17ms)` |
| scrolling_bottom_small_region | `18.65ms avg (90% < 22ms)` | `14.66ms avg (90% < 17ms)` |
| scrolling_fullscreen | `26.6ms avg (90% < 30ms)` | `23.16ms avg (90% < 25ms)` |
| scrolling_top_region | `36.05ms avg (90% < 40ms)` | `32.05ms avg (90% < 35ms)` |
| scrolling_top_small_region | `18.83ms avg (90% < 23ms)` | `14.62ms avg (90% < 17ms)` |
| sync_medium_cells | `16.66ms avg (90% < 20ms)` | `16.29ms avg (90% < 18ms)` |
| unicode | `11.02ms avg (90% < 15ms)` | `7.97ms avg (90% < 10ms)` |

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
