#!/usr/bin/env bash

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
results_dir="${CUTTY_PERF_RESULTS_DIR:-${repo_dir}/target/bench-results/headless-render}"
raw_log="${results_dir}/raw.log"
report="${results_dir}/report.md"

cd "${repo_dir}"
mkdir -p "${results_dir}"
cargo test --release -p cutty display::bench::render_perf -- --ignored --nocapture 2>&1 \
    | tee "${raw_log}"

awk '
    /^# CuTTY Headless Performance Benchmark$/ { capture = 1 }
    /^CUTTY_BENCHMARK_END$/ { capture = 0 }
    capture { print }
' "${raw_log}" > "${report}"

printf 'Saved raw log: %s\n' "${raw_log}"
printf 'Saved report: %s\n' "${report}"
