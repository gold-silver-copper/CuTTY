#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
source "${SCRIPT_DIR}/benchmark_common.sh"

DEFAULT_RESULTS_DIR="${REPO_ROOT}/target/kitten-benchmarks"
DEFAULT_CUTTY_BIN="${REPO_ROOT}/target/release/cutty"
DEFAULT_CUTTY_DEBUG_BIN="${REPO_ROOT}/target/debug/cutty"
DEFAULT_ALACRITTY_APP="/Applications/Alacritty.app/Contents/MacOS/alacritty"
DEFAULT_KITTEN_APP="/Applications/kitty.app/Contents/MacOS/kitten"
DEFAULT_TIMEOUT=1800

usage() {
    cat <<'EOF'
Usage: run-kitten-benchmark.sh [options]

Launch CuTTY and Alacritty, run kitty's builtin throughput benchmark inside
each terminal, save the logs, and compile a report with a winner for every
benchmark category.

Options:
  --cutty-bin PATH      Path to the CuTTY binary.
  --alacritty-bin PATH  Path to the Alacritty binary.
  --kitten-bin PATH     Path to the `kitten` binary.
  --results-dir PATH    Directory for benchmark logs and reports.
                        Default: ./target/kitten-benchmarks
  --render              Pass `--render` to `kitten __benchmark__`.
  --timeout-seconds N   Max time to wait for both benchmarks. Default: 1800.
  -h, --help            Show this help.
EOF
}

CUTTY_BIN=""
ALACRITTY_BIN=""
KITTEN_BIN=""
RESULTS_DIR="${DEFAULT_RESULTS_DIR}"
RENDER_FLAG=0
TIMEOUT_SECONDS="${DEFAULT_TIMEOUT}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --cutty-bin)
            CUTTY_BIN="$2"
            shift 2
            ;;
        --alacritty-bin)
            ALACRITTY_BIN="$2"
            shift 2
            ;;
        --kitten-bin)
            KITTEN_BIN="$2"
            shift 2
            ;;
        --results-dir)
            RESULTS_DIR="$2"
            shift 2
            ;;
        --render)
            RENDER_FLAG=1
            shift
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
KITTEN_BIN="$(benchmark_resolve_binary "${KITTEN_BIN}" "${DEFAULT_KITTEN_APP}" "kitten")"
PYTHON_BIN="$(benchmark_resolve_binary "" "" "python3")"

rm -f \
    "${RESULTS_DIR}/cutty.done" "${RESULTS_DIR}/cutty.status" \
    "${RESULTS_DIR}/alacritty.done" "${RESULTS_DIR}/alacritty.status"

CHILD_SCRIPT="${SCRIPT_DIR}/benchmark_child.sh"
REPORT_SCRIPT="${SCRIPT_DIR}/benchmark_report.py"

child_args=(
    --mode kitten
    --results-dir "${RESULTS_DIR}"
    --kitten-bin "${KITTEN_BIN}"
)
if (( RENDER_FLAG )); then
    child_args+=(--render)
fi

benchmark_launch_terminal \
    "CuTTY" \
    "${CUTTY_BIN}" \
    -e bash "${CHILD_SCRIPT}" \
    "${child_args[@]}" \
    --label cutty

benchmark_status "Waiting for CuTTY kitty benchmark run to finish"
benchmark_wait_for_markers "${RESULTS_DIR}" "${TIMEOUT_SECONDS}" cutty
benchmark_check_status_files "${RESULTS_DIR}" cutty

benchmark_launch_terminal \
    "Alacritty" \
    "${ALACRITTY_BIN}" \
    -e bash "${CHILD_SCRIPT}" \
    "${child_args[@]}" \
    --label alacritty

benchmark_status "Waiting for Alacritty kitty benchmark run to finish"
benchmark_wait_for_markers "${RESULTS_DIR}" "${TIMEOUT_SECONDS}" alacritty
benchmark_check_status_files "${RESULTS_DIR}" alacritty

LOG_SUFFIX=""
if (( RENDER_FLAG )); then
    LOG_SUFFIX="-render"
fi

REPORT_FILE="${RESULTS_DIR}/report${LOG_SUFFIX}.md"
"${PYTHON_BIN}" "${REPORT_SCRIPT}" \
    kitten \
    --cutty-log "${RESULTS_DIR}/cutty${LOG_SUFFIX}.log" \
    --alacritty-log "${RESULTS_DIR}/alacritty${LOG_SUFFIX}.log" \
    --output "${REPORT_FILE}"

echo
echo "Saved CuTTY log: ${RESULTS_DIR}/cutty${LOG_SUFFIX}.log"
echo "Saved Alacritty log: ${RESULTS_DIR}/alacritty${LOG_SUFFIX}.log"
echo "Saved report: ${REPORT_FILE}"
