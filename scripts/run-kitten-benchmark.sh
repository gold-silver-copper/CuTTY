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
  --terminals LIST      Comma-separated terminals to test.
                        Supported: cutty,alacritty,kitty,ghostty
                        Default: all terminals
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

resolve_terminal_binary() {
    local terminal="$1"
    case "${terminal}" in
        cutty)
            if [[ -n "${CUTTY_BIN}" ]]; then
                benchmark_resolve_binary "${CUTTY_BIN}" "" ""
            elif [[ -x "${DEFAULT_CUTTY_BIN}" ]]; then
                printf '%s\n' "${DEFAULT_CUTTY_BIN}"
            elif [[ -x "${DEFAULT_CUTTY_DEBUG_BIN}" ]]; then
                printf '%s\n' "${DEFAULT_CUTTY_DEBUG_BIN}"
            else
                benchmark_fail "unable to find a CuTTY binary; tried ${DEFAULT_CUTTY_BIN} and ${DEFAULT_CUTTY_DEBUG_BIN}"
            fi
            ;;
        alacritty)
            benchmark_resolve_binary "${ALACRITTY_BIN}" "${DEFAULT_ALACRITTY_APP}" "alacritty"
            ;;
        kitty)
            benchmark_resolve_binary "${KITTY_BIN}" "${DEFAULT_KITTY_APP}" "kitty"
            ;;
        ghostty)
            benchmark_resolve_binary "${GHOSTTY_BIN}" "${DEFAULT_GHOSTTY_APP}" "ghostty"
            ;;
        *)
            benchmark_fail "unsupported terminal kind: ${terminal}"
            ;;
    esac
}

CUTTY_BIN=""
ALACRITTY_BIN=""
KITTY_BIN=""
GHOSTTY_BIN=""
KITTEN_BIN=""
TERMINALS=""
RESULTS_DIR="${DEFAULT_RESULTS_DIR}"
RENDER_FLAG=0
TIMEOUT_SECONDS="${DEFAULT_TIMEOUT}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --terminals)
            TERMINALS="$2"
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

KITTEN_BIN="$(benchmark_resolve_binary "${KITTEN_BIN}" "${DEFAULT_KITTEN_APP}" "kitten")"
PYTHON_BIN="$(benchmark_resolve_binary "" "" "python3")"

CHILD_SCRIPT="${SCRIPT_DIR}/benchmark_child.sh"
REPORT_SCRIPT="${SCRIPT_DIR}/benchmark_report.py"
LOG_SUFFIX=""
if (( RENDER_FLAG )); then
    LOG_SUFFIX="-render"
fi

mapfile -t SELECTED_TERMINALS < <(benchmark_parse_terminals "${TERMINALS}")
terminal_logs=()

for terminal_kind in "${SELECTED_TERMINALS[@]}"; do
    label="${terminal_kind}"
    display_name="$(benchmark_terminal_display_name "${terminal_kind}")"
    terminal_bin="$(resolve_terminal_binary "${terminal_kind}")"
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
    terminal_logs+=("--terminal-log" "${display_name}=${RESULTS_DIR}/${label}${LOG_SUFFIX}.log")
done

REPORT_FILE="${RESULTS_DIR}/report${LOG_SUFFIX}.md"
benchmark_status "Generating kitty benchmark report"
"${PYTHON_BIN}" "${REPORT_SCRIPT}" \
    kitten \
    "${terminal_logs[@]}" \
    --output "${REPORT_FILE}"

echo
for terminal_kind in "${SELECTED_TERMINALS[@]}"; do
    display_name="$(benchmark_terminal_display_name "${terminal_kind}")"
    echo "Saved ${display_name} log: ${RESULTS_DIR}/${terminal_kind}${LOG_SUFFIX}.log"
done
echo "Saved report: ${REPORT_FILE}"
