#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
source "${SCRIPT_DIR}/benchmark_common.sh"

DEFAULT_RESULTS_DIR="${REPO_ROOT}/target/kitten-benchmarks"
DEFAULT_CUTTY_BIN="${REPO_ROOT}/target/release/cutty"
DEFAULT_CUTTY_DEBUG_BIN="${REPO_ROOT}/target/debug/cutty"
DEFAULT_ALACRITTY_APP="/Applications/Alacritty.app/Contents/MacOS/alacritty"
DEFAULT_KITTY_APP="/Applications/kitty.app/Contents/MacOS/kitty"
DEFAULT_GHOSTTY_APP="/Applications/Ghostty.app/Contents/MacOS/ghostty"
DEFAULT_KITTEN_APP="/Applications/kitty.app/Contents/MacOS/kitten"
DEFAULT_TIMEOUT=1800

usage() {
    cat <<'EOF'
Usage: run-kitten-benchmark.sh [options]

Launch CuTTY, Alacritty, Kitty, and Ghostty, run kitty's builtin throughput
benchmark inside each terminal, save the logs, and compile a report with a
winner for every benchmark category.

Options:
  --cutty-bin PATH      Path to the CuTTY binary.
  --alacritty-bin PATH  Path to the Alacritty binary.
  --kitty-bin PATH      Path to the Kitty terminal binary.
  --ghostty-bin PATH    Path to the Ghostty terminal binary.
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
KITTY_BIN=""
GHOSTTY_BIN=""
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
        --kitty-bin)
            KITTY_BIN="$2"
            shift 2
            ;;
        --ghostty-bin)
            GHOSTTY_BIN="$2"
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
KITTY_BIN="$(benchmark_resolve_binary "${KITTY_BIN}" "${DEFAULT_KITTY_APP}" "kitty")"
GHOSTTY_BIN="$(benchmark_resolve_binary "${GHOSTTY_BIN}" "${DEFAULT_GHOSTTY_APP}" "ghostty")"
KITTEN_BIN="$(benchmark_resolve_binary "${KITTEN_BIN}" "${DEFAULT_KITTEN_APP}" "kitten")"
PYTHON_BIN="$(benchmark_resolve_binary "" "" "python3")"

CHILD_SCRIPT="${SCRIPT_DIR}/benchmark_child.sh"
REPORT_SCRIPT="${SCRIPT_DIR}/benchmark_report.py"
LOG_SUFFIX=""
if (( RENDER_FLAG )); then
    LOG_SUFFIX="-render"
fi

terminal_specs=(
    "cutty|CuTTY|cutty|${CUTTY_BIN}"
    "alacritty|Alacritty|alacritty|${ALACRITTY_BIN}"
    "kitty|Kitty|kitty|${KITTY_BIN}"
    "ghostty|Ghostty|ghostty|${GHOSTTY_BIN}"
)

for spec in "${terminal_specs[@]}"; do
    IFS="|" read -r label display_name terminal_kind terminal_bin <<< "${spec}"
    rm -f "${RESULTS_DIR}/${label}.done" "${RESULTS_DIR}/${label}.status"

    child_args=(
        bash "${CHILD_SCRIPT}"
        --mode kitten
        --results-dir "${RESULTS_DIR}"
        --kitten-bin "${KITTEN_BIN}"
        --label "${label}"
    )
    if (( RENDER_FLAG )); then
        child_args+=(--render)
    fi

    benchmark_launch_terminal "${terminal_kind}" "${display_name}" "${terminal_bin}" "${child_args[@]}"
    benchmark_status "Waiting for ${display_name} kitty benchmark run to finish"
    benchmark_wait_for_markers "${RESULTS_DIR}" "${TIMEOUT_SECONDS}" "${label}"
    benchmark_check_status_files "${RESULTS_DIR}" "${label}"
done

REPORT_FILE="${RESULTS_DIR}/report${LOG_SUFFIX}.md"
benchmark_status "Generating kitty benchmark report"
"${PYTHON_BIN}" "${REPORT_SCRIPT}" \
    kitten \
    --terminal-log "CuTTY=${RESULTS_DIR}/cutty${LOG_SUFFIX}.log" \
    --terminal-log "Alacritty=${RESULTS_DIR}/alacritty${LOG_SUFFIX}.log" \
    --terminal-log "Kitty=${RESULTS_DIR}/kitty${LOG_SUFFIX}.log" \
    --terminal-log "Ghostty=${RESULTS_DIR}/ghostty${LOG_SUFFIX}.log" \
    --output "${REPORT_FILE}"

echo
echo "Saved CuTTY log: ${RESULTS_DIR}/cutty${LOG_SUFFIX}.log"
echo "Saved Alacritty log: ${RESULTS_DIR}/alacritty${LOG_SUFFIX}.log"
echo "Saved Kitty log: ${RESULTS_DIR}/kitty${LOG_SUFFIX}.log"
echo "Saved Ghostty log: ${RESULTS_DIR}/ghostty${LOG_SUFFIX}.log"
echo "Saved report: ${REPORT_FILE}"
