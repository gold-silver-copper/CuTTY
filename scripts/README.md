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

Run the helper script once inside CuTTY and once inside Alacritty. It executes
`vtebench` in the current terminal, writes a labeled `.log` and `.dat`, and
generates a comparison plot automatically once both `cutty` and `alacritty`
results exist.

```sh
./compare-vtebench-macos.sh --vtebench-dir /path/to/vtebench --label cutty
./compare-vtebench-macos.sh --vtebench-dir /path/to/vtebench --label alacritty
```

## Kitty Throughput Benchmark

Kitty's official throughput benchmark is run with `kitten __benchmark__` inside
the terminal being tested. The helper below wraps that command, saves a labeled
log, and can optionally enable rendering with `--render`.

```sh
./run-kitten-benchmark.sh --label cutty
./run-kitten-benchmark.sh --label alacritty
./run-kitten-benchmark.sh --label cutty --render
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
