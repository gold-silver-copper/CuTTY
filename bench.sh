#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS_DIR="${ROOT_DIR}/scripts"
DEFAULT_RESULTS_DIR="${ROOT_DIR}/target/bench-results"

usage() {
    cat <<'EOF'
Usage: ./bench.sh --vtebench-dir PATH [options]

Run the kitty benchmark and vtebench end-to-end. The script launches CuTTY,
Alacritty, Kitty, and Ghostty sequentially for each benchmark, captures the
logs, generates winner reports, and prints both reports at the end.

Options:
  --vtebench-dir PATH   Path to a local alacritty/vtebench checkout.
  --cutty-bin PATH      Path to the CuTTY binary.
  --alacritty-bin PATH  Path to the Alacritty binary.
  --kitty-bin PATH      Path to the Kitty terminal binary.
  --ghostty-bin PATH    Path to the Ghostty terminal binary.
  --kitten-bin PATH     Path to the `kitten` binary.
  --results-dir PATH    Root directory for all benchmark artifacts.
                        Default: ./target/bench-results
  --render              Pass `--render` to kitty's benchmark.
  --timeout-seconds N   Max time to wait per benchmark run. Default: 1800.
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

VTEBENCH_DIR=""
CUTTY_BIN=""
ALACRITTY_BIN=""
KITTY_BIN=""
GHOSTTY_BIN=""
KITTEN_BIN=""
RESULTS_DIR="${DEFAULT_RESULTS_DIR}"
RENDER_FLAG=0
TIMEOUT_SECONDS=1800

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
            fail "Unknown argument: $1"
            ;;
    esac
done

[[ -n "${VTEBENCH_DIR}" ]] || fail "--vtebench-dir is required"

KITTEN_RESULTS_DIR="${RESULTS_DIR}/kitten"
VTEBENCH_RESULTS_DIR="${RESULTS_DIR}/vtebench"

mkdir -p "${KITTEN_RESULTS_DIR}" "${VTEBENCH_RESULTS_DIR}"

kitten_cmd=(
    bash "${SCRIPTS_DIR}/run-kitten-benchmark.sh"
    --results-dir "${KITTEN_RESULTS_DIR}"
    --timeout-seconds "${TIMEOUT_SECONDS}"
)
vtebench_cmd=(
    bash "${SCRIPTS_DIR}/compare-vtebench-macos.sh"
    --vtebench-dir "${VTEBENCH_DIR}"
    --results-dir "${VTEBENCH_RESULTS_DIR}"
    --timeout-seconds "${TIMEOUT_SECONDS}"
)

if [[ -n "${CUTTY_BIN}" ]]; then
    kitten_cmd+=(--cutty-bin "${CUTTY_BIN}")
    vtebench_cmd+=(--cutty-bin "${CUTTY_BIN}")
fi

if [[ -n "${ALACRITTY_BIN}" ]]; then
    kitten_cmd+=(--alacritty-bin "${ALACRITTY_BIN}")
    vtebench_cmd+=(--alacritty-bin "${ALACRITTY_BIN}")
fi

if [[ -n "${KITTY_BIN}" ]]; then
    kitten_cmd+=(--kitty-bin "${KITTY_BIN}")
    vtebench_cmd+=(--kitty-bin "${KITTY_BIN}")
fi

if [[ -n "${GHOSTTY_BIN}" ]]; then
    kitten_cmd+=(--ghostty-bin "${GHOSTTY_BIN}")
    vtebench_cmd+=(--ghostty-bin "${GHOSTTY_BIN}")
fi

if [[ -n "${KITTEN_BIN}" ]]; then
    kitten_cmd+=(--kitten-bin "${KITTEN_BIN}")
fi

if (( RENDER_FLAG )); then
    kitten_cmd+=(--render)
fi

status "Running kitty benchmark suite"
"${kitten_cmd[@]}"

status "Running vtebench suite"
"${vtebench_cmd[@]}"

KITTEN_REPORT="${KITTEN_RESULTS_DIR}/report"
if (( RENDER_FLAG )); then
    KITTEN_REPORT+="-render"
fi
KITTEN_REPORT+=".md"
VTEBENCH_REPORT="${VTEBENCH_RESULTS_DIR}/report.md"

[[ -f "${KITTEN_REPORT}" ]] || fail "missing kitty report: ${KITTEN_REPORT}"
[[ -f "${VTEBENCH_REPORT}" ]] || fail "missing vtebench report: ${VTEBENCH_REPORT}"

echo
echo "=== Kitty Benchmark Results ==="
cat "${KITTEN_REPORT}"

echo
echo "=== vtebench Results ==="
cat "${VTEBENCH_REPORT}"

echo
echo "Artifacts:"
echo "  Kitty results: ${KITTEN_RESULTS_DIR}"
echo "  vtebench results: ${VTEBENCH_RESULTS_DIR}"
