#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_RESULTS_ROOT="${REPO_ROOT}/target/vtebench-results"

usage() {
    cat <<'EOF'
Usage: compare-vtebench-macos.sh --vtebench-dir PATH --label NAME [options]

Run vtebench in the current terminal and save the output under a label like
"cutty" or "alacritty".

Run this script once inside CuTTY:
  ./compare-vtebench-macos.sh --vtebench-dir /path/to/vtebench --label cutty

Then run it again inside Alacritty:
  ./compare-vtebench-macos.sh --vtebench-dir /path/to/vtebench --label alacritty

Options:
  --vtebench-dir PATH   Path to a local alacritty/vtebench checkout.
  --label NAME          Result label, usually "cutty" or "alacritty".
  --results-dir PATH    Directory for logs, .dat files, and plots.
                        Default: ./target/vtebench-results
  -h, --help            Show this help.
EOF
}

fail() {
    echo "error: $*" >&2
    exit 1
}

status() {
    echo "[$(date +"%Y-%m-%d %H:%M:%S")] $*"
}

require_file() {
    local path="$1"
    local description="$2"

    [[ -f "${path}" ]] || fail "${description} not found: ${path}"
}

require_dir() {
    local path="$1"
    local description="$2"

    [[ -d "${path}" ]] || fail "${description} not found: ${path}"
}

VTEBENCH_DIR=""
LABEL=""
RESULTS_DIR="${DEFAULT_RESULTS_ROOT}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --vtebench-dir)
            VTEBENCH_DIR="$2"
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
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "Unknown argument: $1"
            ;;
    esac
done

[[ -n "${VTEBENCH_DIR}" ]] || fail "--vtebench-dir is required"
[[ -n "${LABEL}" ]] || fail "--label is required"

require_dir "${VTEBENCH_DIR}" "vtebench directory"
require_file "${VTEBENCH_DIR}/Cargo.toml" "vtebench Cargo manifest"
require_dir "${VTEBENCH_DIR}/benchmarks" "vtebench benchmarks directory"

mkdir -p "${RESULTS_DIR}"

LOG_FILE="${RESULTS_DIR}/${LABEL}.log"
DAT_FILE="${RESULTS_DIR}/${LABEL}.dat"
PLOT_FILE="${RESULTS_DIR}/comparison.svg"

status "Running vtebench in the current terminal"
status "label: ${LABEL}"
status "vtebench: ${VTEBENCH_DIR}"
status "results: ${RESULTS_DIR}"
status "log: ${LOG_FILE}"
status "dat: ${DAT_FILE}"
echo

(
    cd "${VTEBENCH_DIR}"
    cargo run --release -- --dat "${DAT_FILE}"
) 2>&1 | tee "${LOG_FILE}"

echo
status "Finished vtebench run for ${LABEL}"
echo "Saved log: ${LOG_FILE}"
echo "Saved dat: ${DAT_FILE}"

if [[ -f "${RESULTS_DIR}/cutty.dat" && -f "${RESULTS_DIR}/alacritty.dat" ]]; then
    if [[ -x "${VTEBENCH_DIR}/gnuplot/summary.sh" ]]; then
        "${VTEBENCH_DIR}/gnuplot/summary.sh" \
            "${RESULTS_DIR}/cutty.dat" \
            "${RESULTS_DIR}/alacritty.dat" \
            "${PLOT_FILE}" >/dev/null 2>&1 || true
        if [[ -f "${PLOT_FILE}" ]]; then
            echo "Saved comparison plot: ${PLOT_FILE}"
        fi
    fi

    echo
    echo "Both result sets are present."
    echo "CuTTY dat: ${RESULTS_DIR}/cutty.dat"
    echo "Alacritty dat: ${RESULTS_DIR}/alacritty.dat"
else
    echo
    echo "Comparison is not ready yet."
    echo "Run this script again in the other terminal with the matching label."
    echo "Expected labels: cutty and alacritty"
fi
