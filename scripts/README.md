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
