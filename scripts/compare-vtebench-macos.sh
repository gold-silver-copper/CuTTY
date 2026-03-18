#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
source "${SCRIPT_DIR}/benchmark_common.sh"

DEFAULT_RESULTS_ROOT="${REPO_ROOT}/target/vtebench-results"
DEFAULT_CUTTY_BIN="${REPO_ROOT}/target/release/cutty"
DEFAULT_CUTTY_DEBUG_BIN="${REPO_ROOT}/target/debug/cutty"
DEFAULT_ALACRITTY_APP="/Applications/Alacritty.app/Contents/MacOS/alacritty"
DEFAULT_TIMEOUT=1800

usage() {
    cat <<'EOF'
Usage: compare-vtebench-macos.sh --vtebench-dir PATH [options]

Launch CuTTY and Alacritty, run vtebench inside each terminal, save the logs,
and compile a report with a winner for every benchmark category.

Options:
  --vtebench-dir PATH   Path to a local alacritty/vtebench checkout.
  --cutty-bin PATH      Path to the CuTTY binary.
  --alacritty-bin PATH  Path to the Alacritty binary.
  --results-dir PATH    Directory for logs, .dat files, plots, and reports.
                        Default: ./target/vtebench-results
  --timeout-seconds N   Max time to wait for both benchmarks. Default: 1800.
  -h, --help            Show this help.
EOF
}

VTEBENCH_DIR=""
CUTTY_BIN=""
ALACRITTY_BIN=""
RESULTS_DIR="${DEFAULT_RESULTS_ROOT}"
TIMEOUT_SECONDS="${DEFAULT_TIMEOUT}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --vtebench-dir)
            VTEBENCH_DIR="$2"
            shift 2
            ;;
        --cutty-bin)
            CUTTY_BIN="$2"
            shift 2
            ;;
        --alacritty-bin)
            ALACRITTY_BIN="$2"
            shift 2
            ;;
        --results-dir)
            RESULTS_DIR="$2"
            shift 2
            ;;
        --timeout-seconds)
            TIMEOUT_SECONDS="$2"
            shift 2
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

[[ -n "${VTEBENCH_DIR}" ]] || benchmark_fail "--vtebench-dir is required"
benchmark_require_dir "${VTEBENCH_DIR}" "vtebench directory"
benchmark_require_file "${VTEBENCH_DIR}/Cargo.toml" "vtebench Cargo manifest"
benchmark_require_dir "${VTEBENCH_DIR}/benchmarks" "vtebench benchmarks directory"

mkdir -p "${RESULTS_DIR}"

if [[ -n "${CUTTY_BIN}" ]]; then
    CUTTY_BIN="$(benchmark_resolve_binary "${CUTTY_BIN}" "" "")"
elif [[ -x "${DEFAULT_CUTTY_BIN}" ]]; then
    CUTTY_BIN="${DEFAULT_CUTTY_BIN}"
elif [[ -x "${DEFAULT_CUTTY_DEBUG_BIN}" ]]; then
    CUTTY_BIN="${DEFAULT_CUTTY_DEBUG_BIN}"
else
    benchmark_fail "unable to find a CuTTY binary; tried ${DEFAULT_CUTTY_BIN} and ${DEFAULT_CUTTY_DEBUG_BIN}"
fi
ALACRITTY_BIN="$(benchmark_resolve_binary "${ALACRITTY_BIN}" "${DEFAULT_ALACRITTY_APP}" "alacritty")"
PYTHON_BIN="$(benchmark_resolve_binary "" "" "python3")"

rm -f \
    "${RESULTS_DIR}/cutty.done" "${RESULTS_DIR}/cutty.status" \
    "${RESULTS_DIR}/alacritty.done" "${RESULTS_DIR}/alacritty.status"

CHILD_SCRIPT="${SCRIPT_DIR}/benchmark_child.sh"
REPORT_SCRIPT="${SCRIPT_DIR}/benchmark_report.py"

benchmark_launch_terminal \
    "CuTTY" \
    "${CUTTY_BIN}" \
    -e bash "${CHILD_SCRIPT}" \
    --mode vtebench \
    --label cutty \
    --results-dir "${RESULTS_DIR}" \
    --vtebench-dir "${VTEBENCH_DIR}"

benchmark_status "Waiting for CuTTY vtebench run to finish"
benchmark_wait_for_markers "${RESULTS_DIR}" "${TIMEOUT_SECONDS}" cutty
benchmark_check_status_files "${RESULTS_DIR}" cutty

benchmark_launch_terminal \
    "Alacritty" \
    "${ALACRITTY_BIN}" \
    -e bash "${CHILD_SCRIPT}" \
    --mode vtebench \
    --label alacritty \
    --results-dir "${RESULTS_DIR}" \
    --vtebench-dir "${VTEBENCH_DIR}"

benchmark_status "Waiting for Alacritty vtebench run to finish"
benchmark_wait_for_markers "${RESULTS_DIR}" "${TIMEOUT_SECONDS}" alacritty
benchmark_check_status_files "${RESULTS_DIR}" alacritty

PLOT_FILE="${RESULTS_DIR}/comparison.svg"
if [[ -x "${VTEBENCH_DIR}/gnuplot/summary.sh" ]]; then
    "${VTEBENCH_DIR}/gnuplot/summary.sh" \
        "${RESULTS_DIR}/cutty.dat" \
        "${RESULTS_DIR}/alacritty.dat" \
        "${PLOT_FILE}" >/dev/null 2>&1 || true
fi

REPORT_FILE="${RESULTS_DIR}/report.md"
"${PYTHON_BIN}" "${REPORT_SCRIPT}" \
    vtebench \
    --cutty-log "${RESULTS_DIR}/cutty.log" \
    --alacritty-log "${RESULTS_DIR}/alacritty.log" \
    --output "${REPORT_FILE}"

echo
echo "Saved CuTTY log: ${RESULTS_DIR}/cutty.log"
echo "Saved Alacritty log: ${RESULTS_DIR}/alacritty.log"
echo "Saved CuTTY dat: ${RESULTS_DIR}/cutty.dat"
echo "Saved Alacritty dat: ${RESULTS_DIR}/alacritty.dat"
if [[ -f "${PLOT_FILE}" ]]; then
    echo "Saved comparison plot: ${PLOT_FILE}"
fi
echo "Saved report: ${REPORT_FILE}"
