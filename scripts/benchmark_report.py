#!/usr/bin/env python3

from __future__ import annotations

import argparse
import pathlib
import re
import sys

ANSI_ESCAPE = re.compile(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\].*?(?:\x07|\x1b\\)|[@-Z\\-_])")

KITTEN_LINE = re.compile(
    r"^\s*(?P<name>.+?)\s*:\s*(?P<seconds>[0-9.]+)s\s*@\s*(?P<mbps>[0-9.]+)\s+MB/s\s*$"
)
VTEBENCH_HEADER = re.compile(
    r"^\s*(?P<name>[A-Za-z0-9_]+)\s+\((?P<samples>\d+) samples @ (?P<size>[^)]+)\):\s*$"
)
VTEBENCH_METRICS = re.compile(
    r"^\s*(?P<avg>[0-9.]+)ms avg \(90% < (?P<p90>[0-9.]+)ms\) \+\-(?P<sigma>[0-9.]+)ms\s*$"
)


def parse_kitten_log(path: pathlib.Path) -> dict[str, dict[str, float]]:
    results: dict[str, dict[str, float]] = {}
    for raw_line in path.read_text().splitlines():
        line = ANSI_ESCAPE.sub("", raw_line)
        match = KITTEN_LINE.match(line)
        if not match:
            continue
        results[match.group("name")] = {
            "seconds": float(match.group("seconds")),
            "mbps": float(match.group("mbps")),
        }
    if not results:
        raise ValueError(f"no kitty benchmark results found in {path}")
    return results


def parse_vtebench_log(path: pathlib.Path) -> dict[str, dict[str, float | str]]:
    results: dict[str, dict[str, float | str]] = {}
    pending_name: str | None = None
    pending_size: str | None = None
    pending_samples: int | None = None

    for raw_line in path.read_text().splitlines():
        line = ANSI_ESCAPE.sub("", raw_line)
        header = VTEBENCH_HEADER.match(line)
        if header:
            pending_name = header.group("name")
            pending_size = header.group("size")
            pending_samples = int(header.group("samples"))
            continue

        if pending_name is None:
            continue

        metrics = VTEBENCH_METRICS.match(line)
        if metrics:
            results[pending_name] = {
                "samples": pending_samples,
                "size": pending_size,
                "avg_ms": float(metrics.group("avg")),
                "p90_ms": float(metrics.group("p90")),
                "sigma_ms": float(metrics.group("sigma")),
            }
            pending_name = None
            pending_size = None
            pending_samples = None

    if not results:
        raise ValueError(f"no vtebench results found in {path}")
    return results


def format_kitten_report(cutty: dict[str, dict[str, float]], alacritty: dict[str, dict[str, float]]) -> str:
    names = sorted(set(cutty) | set(alacritty))
    lines = [
        "# Kitty Benchmark Report",
        "",
        "| Test | Alacritty | CuTTY | Winner |",
        "| --- | --- | --- | --- |",
    ]
    for name in names:
        a = alacritty.get(name)
        c = cutty.get(name)
        if a is None or c is None:
            winner = "missing data"
            a_text = format_missing(a)
            c_text = format_missing(c)
        else:
            winner = "CuTTY" if c["mbps"] > a["mbps"] else "Alacritty" if a["mbps"] > c["mbps"] else "Tie"
            a_text = f"`{a['seconds']:.2f}s @ {a['mbps']:.1f} MB/s`"
            c_text = f"`{c['seconds']:.2f}s @ {c['mbps']:.1f} MB/s`"
        lines.append(f"| {name} | {a_text} | {c_text} | {winner} |")
    return "\n".join(lines) + "\n"


def format_vtebench_report(
    cutty: dict[str, dict[str, float | str]],
    alacritty: dict[str, dict[str, float | str]],
) -> str:
    names = sorted(set(cutty) | set(alacritty))
    lines = [
        "# vtebench Report",
        "",
        "| Test | Alacritty | CuTTY | Winner |",
        "| --- | --- | --- | --- |",
    ]
    for name in names:
        a = alacritty.get(name)
        c = cutty.get(name)
        if a is None or c is None:
            winner = "missing data"
            a_text = format_missing(a)
            c_text = format_missing(c)
        else:
            winner = "CuTTY" if c["avg_ms"] < a["avg_ms"] else "Alacritty" if a["avg_ms"] < c["avg_ms"] else "Tie"
            a_text = f"`{a['avg_ms']:.2f}ms avg (90% < {a['p90_ms']:.0f}ms)`"
            c_text = f"`{c['avg_ms']:.2f}ms avg (90% < {c['p90_ms']:.0f}ms)`"
        lines.append(f"| {name} | {a_text} | {c_text} | {winner} |")
    return "\n".join(lines) + "\n"


def format_missing(value: object | None) -> str:
    return "`missing`" if value is None else "`present`"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("kitten", "vtebench"))
    parser.add_argument("--cutty-log", required=True)
    parser.add_argument("--alacritty-log", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    cutty_log = pathlib.Path(args.cutty_log)
    alacritty_log = pathlib.Path(args.alacritty_log)
    output = pathlib.Path(args.output)

    try:
        if args.mode == "kitten":
            report = format_kitten_report(parse_kitten_log(cutty_log), parse_kitten_log(alacritty_log))
        else:
            report = format_vtebench_report(parse_vtebench_log(cutty_log), parse_vtebench_log(alacritty_log))
    except Exception as exc:  # pragma: no cover - simple CLI wrapper
        print(f"error: {exc}", file=sys.stderr)
        return 1

    output.write_text(report)
    print(report, end="")
    print(f"Saved report: {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
