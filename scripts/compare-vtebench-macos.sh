#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
source "${SCRIPT_DIR}/benchmark_common.sh"

DEFAULT_RESULTS_ROOT="${REPO_ROOT}/target/vtebench-results"
DEFAULT_CUTTY_BIN="${REPO_ROOT}/target/release/cutty"
DEFAULT_CUTTY_DEBUG_BIN="${REPO_ROOT}/target/debug/cutty"
DEFAULT_ALACRITTY_APP="/Applications/Alacritty.app/Contents/MacOS/alacritty"
DEFAULT_KITTY_APP="/Applications/kitty.app/Contents/MacOS/kitty"
DEFAULT_GHOSTTY_APP="/Applications/Ghostty.app/Contents/MacOS/ghostty"
DEFAULT_TIMEOUT=1800

usage() {
    cat <<'EOF'
Usage: compare-vtebench-macos.sh --vtebench-dir PATH [options]

Launch CuTTY, Alacritty, Kitty, and Ghostty, run vtebench inside each terminal,
save the logs, and compile a report with a winner for every benchmark category.

Options:
  --vtebench-dir PATH   Path to a local alacritty/vtebench checkout.
  --terminals LIST      Comma-separated terminals to test.
                        Supported: cutty,alacritty,kitty,ghostty
                        Default: all terminals
  --cutty-bin PATH      Path to the CuTTY binary.
  --alacritty-bin PATH  Path to the Alacritty binary.
  --kitty-bin PATH      Path to the Kitty terminal binary.
  --ghostty-bin PATH    Path to the Ghostty terminal binary.
  --results-dir PATH    Directory for logs, .dat files, plots, and reports.
                        Default: ./target/vtebench-results
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

VTEBENCH_DIR=""
CUTTY_BIN=""
ALACRITTY_BIN=""
KITTY_BIN=""
GHOSTTY_BIN=""
TERMINALS=""
RESULTS_DIR="${DEFAULT_RESULTS_ROOT}"
TIMEOUT_SECONDS="${DEFAULT_TIMEOUT}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --vtebench-dir)
            VTEBENCH_DIR="$2"
            shift 2
            ;;
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

PYTHON_BIN="$(benchmark_resolve_binary "" "" "python3")"

CHILD_SCRIPT="${SCRIPT_DIR}/benchmark_child.sh"
REPORT_SCRIPT="${SCRIPT_DIR}/benchmark_report.py"
mapfile -t SELECTED_TERMINALS < <(benchmark_parse_terminals "${TERMINALS}")
terminal_dats=()

for terminal_kind in "${SELECTED_TERMINALS[@]}"; do
    label="${terminal_kind}"
    display_name="$(benchmark_terminal_display_name "${terminal_kind}")"
    terminal_bin="$(resolve_terminal_binary "${terminal_kind}")"
    rm -f "${RESULTS_DIR}/${label}.done" "${RESULTS_DIR}/${label}.status"

    child_args=(
        bash "${CHILD_SCRIPT}"
        --mode vtebench
        --label "${label}"
        --results-dir "${RESULTS_DIR}"
        --vtebench-dir "${VTEBENCH_DIR}"
    )

    benchmark_launch_terminal "${terminal_kind}" "${display_name}" "${terminal_bin}" "${child_args[@]}"
    benchmark_status "Waiting for ${display_name} vtebench run to finish"
    benchmark_wait_for_markers "${RESULTS_DIR}" "${TIMEOUT_SECONDS}" "${label}"
    benchmark_check_status_files "${RESULTS_DIR}" "${label}"
    terminal_dats+=("--terminal-dat" "${display_name}=${RESULTS_DIR}/${label}.dat")
done

PLOT_FILE="${RESULTS_DIR}/comparison.svg"
if [[ ${#SELECTED_TERMINALS[@]} -eq 4 && -x "${VTEBENCH_DIR}/gnuplot/summary.sh" ]]; then
    benchmark_status "Generating vtebench plot"
    "${VTEBENCH_DIR}/gnuplot/summary.sh" \
        "${RESULTS_DIR}/cutty.dat" \
        "${RESULTS_DIR}/alacritty.dat" \
        "${RESULTS_DIR}/kitty.dat" \
        "${RESULTS_DIR}/ghostty.dat" \
        "${PLOT_FILE}" >/dev/null 2>&1 || true
fi

REPORT_FILE="${RESULTS_DIR}/report.md"
benchmark_status "Generating vtebench report"
"${PYTHON_BIN}" "${REPORT_SCRIPT}" \
    vtebench \
    "${terminal_dats[@]}" \
    --output "${REPORT_FILE}"

echo
for terminal_kind in "${SELECTED_TERMINALS[@]}"; do
    display_name="$(benchmark_terminal_display_name "${terminal_kind}")"
    echo "Saved ${display_name} log: ${RESULTS_DIR}/${terminal_kind}.log"
    echo "Saved ${display_name} dat: ${RESULTS_DIR}/${terminal_kind}.dat"
done
if [[ -f "${PLOT_FILE}" ]]; then
    echo "Saved comparison plot: ${PLOT_FILE}"
fi
echo "Saved report: ${REPORT_FILE}"
