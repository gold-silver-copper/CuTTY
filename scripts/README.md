Scripts
=======

## Flamegraph

Run the release version of CuTTY while recording call stacks. After the
CuTTY process exits, a flamegraph will be generated and it's URI printed
as the only output to STDOUT.

```sh
./create-flamegraph.sh
```

Running this script depends on an installation of `perf`.

## vtebench Comparison

This launcher opens CuTTY, Alacritty, Kitty, and Ghostty, runs `vtebench`
inside each terminal, captures a summary `.log` plus the raw `.dat` outputs,
and generates a report with the winner for every benchmark category from the
`.dat` files.

```sh
./compare-vtebench-macos.sh --vtebench-dir /path/to/vtebench
```

## Kitty Throughput Benchmark

This launcher opens CuTTY, Alacritty, Kitty, and Ghostty, runs kitty's official
throughput benchmark inside each terminal, captures the logs, and generates a
report with the winner for every benchmark category.

```sh
./run-kitten-benchmark.sh
./run-kitten-benchmark.sh --render
```

## ANSI Color Tests

We include a few scripts for testing the color of text inside a terminal. The
first shows various foreground and background variants. The second enumerates
all the colors of a standard terminal. The third enumerates the 24-bit colors.

```sh
./fg-bg.sh
./colors.sh
./24-bit-colors.sh
```

## Headless Performance Benchmarks

Run CuTTY's parser/terminal-state and Parley/Vello scene-construction benchmarks without opening a
window or initializing a GPU:

```sh
./scripts/headless-render-bench.sh
```

The ignored release-mode test compares legacy and optimized implementations of glyph batching,
background coalescing, whitespace skipping, retained scenes, cache-first layout lookup, and render
cell scratch-vector reuse. Its thirteen deterministic scene workloads cover 80×24 and 240×67 dense
grids, sparse shell output, source code, logs, mixed Unicode, wide CJK/emoji, combining marks,
colored TUIs, selections, decorations, and worst-case fragmented styles and backgrounds.

The same harness also includes semantic ports of all twelve workloads from vtebench revision
`ead80032e57dee2e75f0b51f2ea67528647d9944` and all five benchmark families from Kitty 0.46.1.
These measure CuTTY's parser and terminal-state handling directly using both a single bulk buffer and
4 KiB PTY-like chunks. The ports preserve cursor movement, scrolling regions, synchronized output,
CSI/OSC traffic, Unicode, and Kitty graphics-protocol streams. Inputs generated randomly upstream
use fixed seeds here, and large recorded fixtures use deterministic generated equivalents so the
harness remains self-contained and reproducible.

CuTTY does not currently implement the Kitty graphics protocol, so the ported `images` case
measures parser scanning and rejection of that APC stream, not image decoding or display.

The harness alternates measurement order, reports paired medians with p10–p90 ranges, validates
glyph counts and workload geometry, and measures both steady-state warm-cache frames and cold-cache
full frames. Parser setup and reset are excluded from parser timings. It writes both a raw log and
Markdown report under
`target/bench-results/headless-render/`. Set `CUTTY_PERF_SAMPLES`, `CUTTY_PERF_WARMUPS`, and
`CUTTY_PERF_TARGET_MS` to trade runtime for precision. Use `CUTTY_PERF_FILTER` to run matching
improvements, suites, or workloads only (for example, `CUTTY_PERF_FILTER=unicode`,
`CUTTY_PERF_FILTER=vtebench`, or `CUTTY_PERF_FILTER=kitty`), or
`CUTTY_PERF_RESULTS_DIR` to choose another output directory.

For absolute cross-build measurements of the native scene path plus every ported parser workload,
run the ignored `display::bench::native_perf` test at each source revision:

```sh
cargo test --release -p cutty display::bench::native_perf -- --ignored --nocapture
```

Extract the Markdown section from each run and compare them with the report helper. Repeating the
same build name merges independent runs by their median:

```sh
python3 ./scripts/benchmark_report.py headless \
  --terminal-log "Installed CuTTY=installed-run-1.md" \
  --terminal-log "Local CuTTY=local-run-1.md" \
  --terminal-log "Local CuTTY=local-run-2.md" \
  --terminal-log "Installed CuTTY=installed-run-2.md" \
  --output report.md
```

An installed application binary does not contain Rust's ignored test harness. For a controlled
comparison, use the commit hash reported by `cutty --version`, compile that source revision with the
same benchmark adapter and toolchain, and compare its native path against the local tree.

## Installed Versus Local CuTTY

Compare the CuTTY application installed in `/Applications` with the current local release build
using both kitty's throughput benchmark and vtebench:

```sh
cargo build --release -p cutty
./scripts/compare-cutty-builds.sh \
  --vtebench-dir /path/to/vtebench
```

The default binaries are `/Applications/CuTTY.app/Contents/MacOS/cutty` and
`./target/release/cutty`. Override them with `--installed-bin` and `--local-bin`. Results are kept
separate from the multi-terminal comparison under `target/cutty-build-comparison/`. Use
`--suite kitty` or `--suite vtebench` to run only one suite.
