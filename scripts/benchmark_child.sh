#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
source "${SCRIPT_DIR}/benchmark_common.sh"

usage() {
    cat <<'EOF'
Usage: benchmark_child.sh --mode MODE --label NAME --results-dir PATH [options]

Options:
  --mode MODE           Either `vtebench` or `kitten`.
  --label NAME          Usually `cutty` or `alacritty`.
  --results-dir PATH    Directory for logs, status files, and outputs.
  --vtebench-dir PATH   Required for `--mode vtebench`.
  --kitten-bin PATH     Required for `--mode kitten`.
  --render              Pass `--render` to `kitten __benchmark__`.
EOF
}

MODE=""
LABEL=""
RESULTS_DIR=""
VTEBENCH_DIR=""
KITTEN_BIN=""
RENDER_FLAG=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --mode)
            MODE="$2"
            shift 2
            ;;
        --label)
            LABEL="$2"
            shift 2
            ;;
        --results-dir)
            RESULTS_DIR="$2"
            shift 2
            ;;
        --vtebench-dir)
            VTEBENCH_DIR="$2"
            shift 2
            ;;
        --kitten-bin)
            KITTEN_BIN="$2"
            shift 2
            ;;
        --render)
            RENDER_FLAG=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            benchmark_fail "Unknown argument: $1"
            ;;
    esac
done

[[ -n "${MODE}" ]] || benchmark_fail "--mode is required"
[[ -n "${LABEL}" ]] || benchmark_fail "--label is required"
[[ -n "${RESULTS_DIR}" ]] || benchmark_fail "--results-dir is required"

mkdir -p "${RESULTS_DIR}"

LOG_SUFFIX=""
if [[ "${MODE}" == "kitten" && "${RENDER_FLAG}" -eq 1 ]]; then
    LOG_SUFFIX="-render"
fi

LOG_FILE="${RESULTS_DIR}/${LABEL}${LOG_SUFFIX}.log"
STATUS_FILE="${RESULTS_DIR}/${LABEL}.status"
DONE_FILE="${RESULTS_DIR}/${LABEL}.done"
DAT_FILE="${RESULTS_DIR}/${LABEL}.dat"

rm -f "${STATUS_FILE}" "${DONE_FILE}"

filter_vtebench_summary() {
    python3 -c '
import re
import sys

ansi = re.compile(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\].*?(?:\x07|\x1b\\\\)|[@-Z\\\\-_])")
header = re.compile(r"^\s{2}[A-Za-z0-9_]+ \(\d+ samples @ [^)]+\):\s*$")
metrics = re.compile(r"^\s{4}[0-9.]+ms avg \(90% < [0-9.]+ms\) \+\-[0-9.]+ms\s*$")

printing = False
pending_blank = False

for raw_line in sys.stdin.buffer:
    line = ansi.sub("", raw_line.decode("utf-8", "ignore")).replace("\r", "").rstrip("\n")
    if line == "Results:":
        printing = True
        pending_blank = False
        print("Results:")
        continue
    if not printing:
        continue
    if not line.strip():
        pending_blank = True
        continue
    if header.match(line) or metrics.match(line):
        if pending_blank:
            print()
            pending_blank = False
        print(line)
' > "${LOG_FILE}"
}

run_vtebench() {
    [[ -n "${VTEBENCH_DIR}" ]] || benchmark_fail "--vtebench-dir is required for vtebench mode"
    benchmark_require_dir "${VTEBENCH_DIR}" "vtebench directory"
    benchmark_require_file "${VTEBENCH_DIR}/Cargo.toml" "vtebench manifest"
    benchmark_require_dir "${VTEBENCH_DIR}/benchmarks" "vtebench benchmarks directory"

    (
        cd "${VTEBENCH_DIR}"
        cargo run --release -- --dat "${DAT_FILE}"
    ) 2>&1 | tee >(filter_vtebench_summary)
}

run_kitten() {
    [[ -n "${KITTEN_BIN}" ]] || benchmark_fail "--kitten-bin is required for kitten mode"

    local -a cmd=("${KITTEN_BIN}" "__benchmark__")
    if (( RENDER_FLAG )); then
        cmd+=("--render")
    fi

    "${cmd[@]}" 2>&1 | tee "${LOG_FILE}"
}

benchmark_status "Starting ${MODE} benchmark for ${LABEL}"
set +e
case "${MODE}" in
    vtebench)
        run_vtebench
        ;;
    kitten)
        run_kitten
        ;;
    *)
        benchmark_fail "unsupported mode: ${MODE}"
        ;;
esac
status=$?
set -e

printf '%s\n' "${status}" > "${STATUS_FILE}"
touch "${DONE_FILE}"

if [[ "${status}" -eq 0 ]]; then
    benchmark_status "Finished ${MODE} benchmark for ${LABEL}"
else
    benchmark_status "${MODE} benchmark for ${LABEL} failed with status ${status}"
fi

exit "${status}"
