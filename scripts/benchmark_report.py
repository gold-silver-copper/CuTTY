#!/usr/bin/env python3

from __future__ import annotations

import argparse
import math
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


def parse_vtebench_dat(path: pathlib.Path) -> dict[str, dict[str, float | str]]:
    lines = [line.strip() for line in path.read_text().splitlines() if line.strip()]
    if not lines:
        raise ValueError(f"no vtebench dat results found in {path}")

    names = lines[0].split()
    if not names:
        raise ValueError(f"invalid vtebench dat header in {path}")

    samples_by_name: dict[str, list[int]] = {name: [] for name in names}
    for line in lines[1:]:
        fields = line.split()
        if len(fields) != len(names):
            raise ValueError(f"invalid vtebench dat row in {path}: {line}")

        for name, value in zip(names, fields):
            if value == "_":
                continue
            samples_by_name[name].append(int(value))

    results: dict[str, dict[str, float | str]] = {}
    for name, samples in samples_by_name.items():
        if not samples:
            continue

        sorted_samples = sorted(samples)
        sample_count = len(samples)
        mean = sum(samples) / sample_count
        percentile_index = max(((sample_count * 90 + 99) // 100) - 1, 0)

        variance = 0.0
        if sample_count > 1:
            variance = sum((sample - mean) ** 2 for sample in samples) / (sample_count - 1)

        results[name] = {
            "samples": sample_count,
            "size": "unknown",
            "avg_ms": mean,
            "p90_ms": float(sorted_samples[percentile_index]),
            "sigma_ms": math.sqrt(variance),
        }

    if not results:
        raise ValueError(f"no vtebench dat samples found in {path}")
    return results


def parse_named_paths(entries: list[str]) -> list[tuple[str, pathlib.Path]]:
    parsed: list[tuple[str, pathlib.Path]] = []
    for entry in entries:
        if "=" not in entry:
            raise ValueError(f"expected NAME=PATH entry, got: {entry}")
        name, raw_path = entry.split("=", 1)
        name = name.strip()
        raw_path = raw_path.strip()
        if not name or not raw_path:
            raise ValueError(f"invalid NAME=PATH entry: {entry}")
        parsed.append((name, pathlib.Path(raw_path)))
    if not parsed:
        raise ValueError("at least one terminal result is required")
    return parsed


def format_missing(value: object | None) -> str:
    return "`missing`" if value is None else "`present`"


def winner_list(
    values: list[tuple[str, float]],
    *,
    higher_is_better: bool,
) -> list[str]:
    if not values:
        return []

    key_fn = max if higher_is_better else min
    best = key_fn(metric for _, metric in values)
    return [name for name, metric in values if metric == best]


def winner_names(
    values: list[tuple[str, float]],
    *,
    higher_is_better: bool,
) -> str:
    winners = winner_list(values, higher_is_better=higher_is_better)
    if not winners:
        return "missing data"
    if len(winners) == 1:
        return winners[0]
    return f"Tie ({', '.join(winners)})"


def format_kitten_cell(value: dict[str, float] | None) -> str:
    if value is None:
        return format_missing(value)
    return f"`{value['seconds']:.2f}s @ {value['mbps']:.1f} MB/s`"


def format_vtebench_cell(value: dict[str, float | str] | None) -> str:
    if value is None:
        return format_missing(value)
    return f"`{value['avg_ms']:.2f}ms avg (90% < {value['p90_ms']:.0f}ms)`"


def render_markdown_table(headers: list[str], rows: list[list[str]]) -> list[str]:
    lines = [
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join("---" for _ in headers) + " |",
    ]
    lines.extend("| " + " | ".join(row) + " |" for row in rows)
    return lines


def format_kitten_report(terminals: list[tuple[str, dict[str, dict[str, float]]]]) -> str:
    test_names = sorted({test for _, results in terminals for test in results})
    headers = ["Test", *[name for name, _ in terminals], "Winner"]
    rows: list[list[str]] = []
    wins = {name: 0 for name, _ in terminals}

    for test_name in test_names:
        row = [test_name]
        metrics: list[tuple[str, float]] = []
        for terminal_name, results in terminals:
            value = results.get(test_name)
            row.append(format_kitten_cell(value))
            if value is not None:
                metrics.append((terminal_name, value["mbps"]))
        winner = winner_names(metrics, higher_is_better=True)
        for winner_name in winner_list(metrics, higher_is_better=True):
            wins[winner_name] += 1
        row.append(winner)
        rows.append(row)

    summary_rows = [[name, str(wins[name])] for name, _ in terminals]
    lines = ["# Kitty Benchmark Report", ""]
    lines.extend(render_markdown_table(headers, rows))
    lines.extend(["", "## Category Wins", ""])
    lines.extend(render_markdown_table(["Terminal", "Wins"], summary_rows))
    return "\n".join(lines) + "\n"


def format_vtebench_report(terminals: list[tuple[str, dict[str, dict[str, float | str]]]]) -> str:
    test_names = sorted({test for _, results in terminals for test in results})
    headers = ["Test", *[name for name, _ in terminals], "Winner"]
    rows: list[list[str]] = []
    wins = {name: 0 for name, _ in terminals}

    for test_name in test_names:
        row = [test_name]
        metrics: list[tuple[str, float]] = []
        for terminal_name, results in terminals:
            value = results.get(test_name)
            row.append(format_vtebench_cell(value))
            if value is not None:
                metrics.append((terminal_name, float(value["avg_ms"])))
        winner = winner_names(metrics, higher_is_better=False)
        for winner_name in winner_list(metrics, higher_is_better=False):
            wins[winner_name] += 1
        row.append(winner)
        rows.append(row)

    summary_rows = [[name, str(wins[name])] for name, _ in terminals]
    lines = ["# vtebench Report", ""]
    lines.extend(render_markdown_table(headers, rows))
    lines.extend(["", "## Category Wins", ""])
    lines.extend(render_markdown_table(["Terminal", "Wins"], summary_rows))
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("kitten", "vtebench"))
    parser.add_argument("--terminal-log", action="append", default=[])
    parser.add_argument("--terminal-dat", action="append", default=[])
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    output = pathlib.Path(args.output)

    try:
        if args.mode == "kitten":
            if not args.terminal_log:
                raise ValueError("kitten mode requires at least one --terminal-log NAME=PATH")
            terminals = [
                (name, parse_kitten_log(path))
                for name, path in parse_named_paths(args.terminal_log)
            ]
            report = format_kitten_report(terminals)
        else:
            if args.terminal_dat:
                terminals = [
                    (name, parse_vtebench_dat(path))
                    for name, path in parse_named_paths(args.terminal_dat)
                ]
            elif args.terminal_log:
                terminals = [
                    (name, parse_vtebench_log(path))
                    for name, path in parse_named_paths(args.terminal_log)
                ]
            else:
                raise ValueError(
                    "vtebench mode requires at least one --terminal-dat NAME=PATH or --terminal-log NAME=PATH"
                )
            report = format_vtebench_report(terminals)
    except Exception as exc:  # pragma: no cover - simple CLI wrapper
        print(f"error: {exc}", file=sys.stderr)
        return 1

    output.write_text(report)
    print(report, end="")
    print(f"Saved report: {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
