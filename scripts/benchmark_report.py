#!/usr/bin/env python3

from __future__ import annotations

import argparse
import math
import pathlib
import re
import statistics
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


def parse_headless_report(path: pathlib.Path) -> dict[str, dict[str, float | str]]:
    results: dict[str, dict[str, float | str]] = {}
    section = ""

    for raw_line in path.read_text().splitlines():
        line = raw_line.strip()
        if line == "## Native Scene Measurements":
            section = "scene"
            continue
        if line == "### External Parser Measurements":
            section = "parser"
            continue
        if line.startswith("#"):
            section = ""
            continue
        if not line.startswith("|") or line.startswith("|---"):
            continue

        fields = [field.strip() for field in line.strip("|").split("|")]
        if section == "scene" and fields[0] != "workload":
            if len(fields) < 7:
                raise ValueError(f"invalid native scene row in {path}: {line}")
            name = fields[0]
            results[f"scene/{name}"] = {
                "suite": "scene",
                "workload": name,
                "mode": "native frame",
                "microseconds": float(fields[3]),
            }
        elif section == "parser" and fields[0] != "suite":
            if len(fields) < 8:
                raise ValueError(f"invalid external parser row in {path}: {line}")
            suite, name = fields[:2]
            results[f"{suite}/{name}/bulk"] = {
                "suite": suite,
                "workload": name,
                "mode": "bulk",
                "microseconds": float(fields[2]),
            }
            results[f"{suite}/{name}/4k"] = {
                "suite": suite,
                "workload": name,
                "mode": "4 KiB chunks",
                "microseconds": float(fields[4]),
            }

    if not results:
        raise ValueError(f"no native headless benchmark results found in {path}")
    return results


def format_headless_report(
    builds: list[tuple[str, dict[str, dict[str, float | str]]]],
    run_counts: dict[str, int] | None = None,
) -> str:
    if len(builds) != 2:
        raise ValueError("headless mode requires exactly two build reports")

    (baseline_name, baseline), (candidate_name, candidate) = builds
    baseline_keys = set(baseline)
    candidate_keys = set(candidate)
    if baseline_keys != candidate_keys:
        missing_candidate = sorted(baseline_keys - candidate_keys)
        missing_baseline = sorted(candidate_keys - baseline_keys)
        raise ValueError(
            "headless reports do not contain identical cases: "
            f"missing from candidate={missing_candidate}, missing from baseline={missing_baseline}"
        )

    group_order = {"scene": 0, "vtebench": 1, "kitty": 2}
    keys = sorted(
        baseline,
        key=lambda key: (
            group_order.get(str(baseline[key]["suite"]), 99),
            str(baseline[key]["workload"]),
            str(baseline[key]["mode"]),
        ),
    )
    rows: list[list[str]] = []
    impacts_by_group: dict[str, list[float]] = {}
    wins = {baseline_name: 0, candidate_name: 0}
    neutral_wins = 0

    for key in keys:
        baseline_value = float(baseline[key]["microseconds"])
        candidate_value = float(candidate[key]["microseconds"])
        impact = 100.0 * (baseline_value - candidate_value) / baseline_value
        ratio = baseline_value / candidate_value
        if impact > 1.0:
            winner = candidate_name
            wins[candidate_name] += 1
        elif impact < -1.0:
            winner = baseline_name
            wins[baseline_name] += 1
        else:
            winner = "Neutral (±1%)"
            neutral_wins += 1

        suite = str(baseline[key]["suite"])
        mode = str(baseline[key]["mode"])
        group = suite if suite == "scene" else f"{suite} {mode}"
        impacts_by_group.setdefault(group, []).append(impact)
        rows.append(
            [
                suite,
                str(baseline[key]["workload"]),
                mode,
                f"{baseline_value:.2f}",
                f"{candidate_value:.2f}",
                f"{impact:+.1f}%",
                f"{ratio:.2f}×",
                winner,
            ]
        )

    summary_rows: list[list[str]] = []
    all_impacts: list[float] = []
    for group, impacts in impacts_by_group.items():
        all_impacts.extend(impacts)
        faster = sum(impact > 1.0 for impact in impacts)
        slower = sum(impact < -1.0 for impact in impacts)
        neutral = len(impacts) - faster - slower
        summary_rows.append(
            [
                group,
                str(len(impacts)),
                str(faster),
                str(neutral),
                str(slower),
                f"{statistics.median(impacts):+.1f}%",
                f"{min(impacts):+.1f}%",
                f"{max(impacts):+.1f}%",
            ]
        )
    summary_rows.append(
        [
            "all cases",
            str(len(all_impacts)),
            str(sum(impact > 1.0 for impact in all_impacts)),
            str(sum(-1.0 <= impact <= 1.0 for impact in all_impacts)),
            str(sum(impact < -1.0 for impact in all_impacts)),
            f"{statistics.median(all_impacts):+.1f}%",
            f"{min(all_impacts):+.1f}%",
            f"{max(all_impacts):+.1f}%",
        ]
    )

    lines = [
        "# CuTTY Installed vs Local Headless Benchmark",
        "",
        f"Baseline: **{baseline_name}**. Candidate: **{candidate_name}**.",
        (
            "These are source revisions compiled with the same toolchain and Rust test harness; "
            "the installed application binary itself does not contain the headless test harness."
        ),
        "Positive local impact means the candidate is faster. Times are per operation.",
        "Results within ±1% are classified as neutral.",
        "",
        "## Summary",
        "",
    ]
    lines.extend(
        render_markdown_table(
            ["Group", "Cases", "Local faster", "Neutral", "Local slower", "Median", "Worst", "Best"],
            summary_rows,
        )
    )
    lines.extend(
        [
            "",
            "## Category Wins",
            "",
            *render_markdown_table(
                ["Build", "Wins"],
                [
                    [baseline_name, str(wins[baseline_name])],
                    [candidate_name, str(wins[candidate_name])],
                    ["Neutral (±1%)", str(neutral_wins)],
                ],
            ),
            "",
            "## Measurements",
            "",
        ]
    )
    lines.extend(
        render_markdown_table(
            [
                "Suite",
                "Workload",
                "Mode",
                f"{baseline_name} µs",
                f"{candidate_name} µs",
                "Local impact",
                "Speedup",
                "Winner",
            ],
            rows,
        )
    )
    if run_counts is not None:
        lines[4:4] = [
            "Measurements are medians across "
            + ", ".join(f"{run_counts[name]} independent {name} runs" for name, _ in builds)
            + "."
        ]
    return "\n".join(lines) + "\n"


def merge_headless_runs(
    runs: list[tuple[str, dict[str, dict[str, float | str]]]],
) -> tuple[list[tuple[str, dict[str, dict[str, float | str]]]], dict[str, int]]:
    grouped: dict[str, list[dict[str, dict[str, float | str]]]] = {}
    order: list[str] = []
    for name, results in runs:
        if name not in grouped:
            grouped[name] = []
            order.append(name)
        grouped[name].append(results)

    merged: list[tuple[str, dict[str, dict[str, float | str]]]] = []
    counts: dict[str, int] = {}
    for name in order:
        reports = grouped[name]
        expected_keys = set(reports[0])
        for report in reports[1:]:
            if set(report) != expected_keys:
                raise ValueError(f"headless runs for {name} do not contain identical cases")

        combined: dict[str, dict[str, float | str]] = {}
        for key in expected_keys:
            combined[key] = dict(reports[0][key])
            combined[key]["microseconds"] = statistics.median(
                float(report[key]["microseconds"]) for report in reports
            )
        merged.append((name, combined))
        counts[name] = len(reports)
    return merged, counts


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("kitten", "vtebench", "headless"))
    parser.add_argument("--terminal-log", action="append", default=[])
    parser.add_argument("--terminal-dat", action="append", default=[])
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    output = pathlib.Path(args.output)

    try:
        if args.mode == "headless":
            runs = [
                (name, parse_headless_report(path))
                for name, path in parse_named_paths(args.terminal_log)
            ]
            terminals, run_counts = merge_headless_runs(runs)
            report = format_headless_report(terminals, run_counts)
        elif args.mode == "kitten":
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
