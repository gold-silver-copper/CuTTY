#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_RESULTS_DIR="${REPO_ROOT}/target/kitten-benchmarks"

usage() {
    cat <<'EOF'
Usage: run-kitten-benchmark.sh --label NAME [options]

Run kitty's builtin throughput benchmark in the current terminal and save the
output under a label such as "cutty" or "alacritty".

Examples:
  ./run-kitten-benchmark.sh --label cutty
  ./run-kitten-benchmark.sh --label alacritty
  ./run-kitten-benchmark.sh --label cutty --render
  ./run-kitten-benchmark.sh --label cutty --kitten-bin /Applications/kitty.app/Contents/MacOS/kitten

Options:
  --label NAME          Result label, usually "cutty" or "alacritty".
  --kitten-bin PATH     Path to the `kitten` binary. Default: `kitten` from PATH.
  --results-dir PATH    Directory for benchmark logs.
                        Default: ./target/kitten-benchmarks
  --render              Pass `--render` to `kitten __benchmark__`.
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

LABEL=""
KITTEN_BIN="kitten"
RESULTS_DIR="${DEFAULT_RESULTS_DIR}"
RENDER_FLAG=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --label)
            LABEL="$2"
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
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "Unknown argument: $1"
            ;;
    esac
done

[[ -n "${LABEL}" ]] || fail "--label is required"

if [[ "${KITTEN_BIN}" == */* ]]; then
    [[ -x "${KITTEN_BIN}" ]] || fail "Kitten binary is not executable: ${KITTEN_BIN}"
elif ! command -v "${KITTEN_BIN}" >/dev/null 2>&1; then
    fail "Unable to find kitten binary on PATH: ${KITTEN_BIN}"
fi

mkdir -p "${RESULTS_DIR}"

LOG_SUFFIX=""
if (( RENDER_FLAG )); then
    LOG_SUFFIX="-render"
fi
LOG_FILE="${RESULTS_DIR}/${LABEL}${LOG_SUFFIX}.log"

status "Running kitty throughput benchmark in the current terminal"
status "label: ${LABEL}"
status "kitten: ${KITTEN_BIN}"
status "results: ${RESULTS_DIR}"
status "log: ${LOG_FILE}"
if (( RENDER_FLAG )); then
    status "mode: render enabled"
else
    status "mode: parser-focused (default, rendering suppressed)"
fi
echo

benchmark_cmd=("${KITTEN_BIN}" "__benchmark__")
if (( RENDER_FLAG )); then
    benchmark_cmd+=("--render")
fi

"${benchmark_cmd[@]}" 2>&1 | tee "${LOG_FILE}"

echo
status "Finished kitty benchmark for ${LABEL}"
echo "Saved log: ${LOG_FILE}"

if [[ -f "${RESULTS_DIR}/cutty.log" && -f "${RESULTS_DIR}/alacritty.log" ]]; then
    echo
    echo "Both default benchmark logs are present."
    echo "CuTTY log: ${RESULTS_DIR}/cutty.log"
    echo "Alacritty log: ${RESULTS_DIR}/alacritty.log"
fi
