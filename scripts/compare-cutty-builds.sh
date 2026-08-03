#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
source "${SCRIPT_DIR}/benchmark_common.sh"

DEFAULT_INSTALLED_BIN="/Applications/CuTTY.app/Contents/MacOS/cutty"
DEFAULT_LOCAL_BIN="${REPO_ROOT}/target/release/cutty"
DEFAULT_KITTEN_BIN="/Applications/kitty.app/Contents/MacOS/kitten"
DEFAULT_RESULTS_DIR="${REPO_ROOT}/target/cutty-build-comparison"
DEFAULT_TIMEOUT=1800

usage() {
    cat <<'EOF'
Usage: compare-cutty-builds.sh --vtebench-dir PATH [options]

Run kitty's throughput benchmark and vtebench in exactly two CuTTY builds:
the version installed in /Applications and the current local release build.

Options:
  --vtebench-dir PATH   Path to a local alacritty/vtebench checkout.
  --installed-bin PATH  Installed CuTTY binary.
                        Default: /Applications/CuTTY.app/Contents/MacOS/cutty
  --local-bin PATH      Local CuTTY release binary.
                        Default: ./target/release/cutty
  --kitten-bin PATH     Path to the `kitten` binary.
  --results-dir PATH    Root directory for logs, data, and reports.
                        Default: ./target/cutty-build-comparison
  --suite SUITE         Suite to run: all, kitty, or vtebench. Default: all.
  --render              Pass `--render` to kitty's benchmark.
  --timeout-seconds N   Maximum time for each terminal run. Default: 1800.
  -h, --help            Show this help.
EOF
}

binary_version() {
    local binary="$1"
    local output
    output="$("${binary}" --version 2>&1)"
    printf '%s\n' "${output%%$'\n'*}"
}

run_child() {
    local mode="$1"
    local label="$2"
    local display_name="$3"
    local terminal_bin="$4"
    local results_dir="$5"

    rm -f "${results_dir}/${label}.done" "${results_dir}/${label}.status"

    local -a child_args=(
        bash "${SCRIPT_DIR}/benchmark_child.sh"
        --mode "${mode}"
        --label "${label}"
        --results-dir "${results_dir}"
    )
    case "${mode}" in
        kitten)
            child_args+=(--kitten-bin "${KITTEN_BIN}")
            if (( RENDER_FLAG )); then
                child_args+=(--render)
            fi
            ;;
        vtebench)
            child_args+=(--vtebench-dir "${VTEBENCH_DIR}")
            ;;
        *)
            benchmark_fail "unsupported benchmark mode: ${mode}"
            ;;
    esac

    benchmark_launch_terminal cutty "${display_name}" "${terminal_bin}" "${child_args[@]}"
    benchmark_status "Waiting for ${display_name} ${mode} run to finish"
    benchmark_wait_for_markers "${results_dir}" "${TIMEOUT_SECONDS}" "${label}"
    benchmark_check_status_files "${results_dir}" "${label}"
}

VTEBENCH_DIR=""
INSTALLED_BIN="${DEFAULT_INSTALLED_BIN}"
LOCAL_BIN="${DEFAULT_LOCAL_BIN}"
KITTEN_BIN="${DEFAULT_KITTEN_BIN}"
RESULTS_DIR="${DEFAULT_RESULTS_DIR}"
SUITE="all"
RENDER_FLAG=0
TIMEOUT_SECONDS="${DEFAULT_TIMEOUT}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --vtebench-dir)
            VTEBENCH_DIR="$2"
            shift 2
            ;;
        --installed-bin)
            INSTALLED_BIN="$2"
            shift 2
            ;;
        --local-bin)
            LOCAL_BIN="$2"
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
        --suite)
            SUITE="$2"
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
            benchmark_fail "unknown argument: $1"
            ;;
    esac
done

case "${SUITE}" in
    all|kitty|vtebench) ;;
    *) benchmark_fail "--suite must be one of: all, kitty, vtebench" ;;
esac

if [[ "${SUITE}" != "kitty" ]]; then
    [[ -n "${VTEBENCH_DIR}" ]] || benchmark_fail "--vtebench-dir is required"
    benchmark_require_dir "${VTEBENCH_DIR}" "vtebench directory"
    benchmark_require_file "${VTEBENCH_DIR}/Cargo.toml" "vtebench Cargo manifest"
    benchmark_require_dir "${VTEBENCH_DIR}/benchmarks" "vtebench benchmarks directory"
fi

INSTALLED_BIN="$(benchmark_resolve_binary "${INSTALLED_BIN}" "" "")"
LOCAL_BIN="$(benchmark_resolve_binary "${LOCAL_BIN}" "" "")"
if cmp -s "${INSTALLED_BIN}" "${LOCAL_BIN}"; then
    benchmark_fail "installed and local CuTTY binaries are identical"
fi

INSTALLED_VERSION="$(binary_version "${INSTALLED_BIN}")"
LOCAL_VERSION="$(binary_version "${LOCAL_BIN}")"
INSTALLED_NAME="Installed CuTTY (${INSTALLED_VERSION})"
LOCAL_NAME="Local CuTTY (${LOCAL_VERSION})"
PYTHON_BIN="$(benchmark_resolve_binary "" "" "python3")"

printf 'Installed binary: %s [%s]\n' "${INSTALLED_BIN}" "${INSTALLED_VERSION}"
printf 'Local binary:     %s [%s]\n' "${LOCAL_BIN}" "${LOCAL_VERSION}"

if [[ "${SUITE}" == "all" || "${SUITE}" == "kitty" ]]; then
    KITTEN_BIN="$(benchmark_resolve_binary "${KITTEN_BIN}" "" "kitten")"
    KITTEN_RESULTS_DIR="${RESULTS_DIR}/kitty"
    mkdir -p "${KITTEN_RESULTS_DIR}"

    benchmark_status "Running kitty benchmark in installed CuTTY"
    run_child kitten installed "${INSTALLED_NAME}" "${INSTALLED_BIN}" "${KITTEN_RESULTS_DIR}"
    benchmark_status "Running kitty benchmark in local CuTTY"
    run_child kitten local "${LOCAL_NAME}" "${LOCAL_BIN}" "${KITTEN_RESULTS_DIR}"

    KITTEN_SUFFIX=""
    if (( RENDER_FLAG )); then
        KITTEN_SUFFIX="-render"
    fi
    KITTEN_REPORT="${KITTEN_RESULTS_DIR}/report${KITTEN_SUFFIX}.md"
    "${PYTHON_BIN}" "${SCRIPT_DIR}/benchmark_report.py" kitten \
        --terminal-log "${INSTALLED_NAME}=${KITTEN_RESULTS_DIR}/installed${KITTEN_SUFFIX}.log" \
        --terminal-log "${LOCAL_NAME}=${KITTEN_RESULTS_DIR}/local${KITTEN_SUFFIX}.log" \
        --output "${KITTEN_REPORT}"
fi

if [[ "${SUITE}" == "all" || "${SUITE}" == "vtebench" ]]; then
    VTEBENCH_RESULTS_DIR="${RESULTS_DIR}/vtebench"
    mkdir -p "${VTEBENCH_RESULTS_DIR}"

    benchmark_status "Running vtebench in installed CuTTY"
    run_child vtebench installed "${INSTALLED_NAME}" "${INSTALLED_BIN}" "${VTEBENCH_RESULTS_DIR}"
    benchmark_status "Running vtebench in local CuTTY"
    run_child vtebench local "${LOCAL_NAME}" "${LOCAL_BIN}" "${VTEBENCH_RESULTS_DIR}"

    VTEBENCH_REPORT="${VTEBENCH_RESULTS_DIR}/report.md"
    "${PYTHON_BIN}" "${SCRIPT_DIR}/benchmark_report.py" vtebench \
        --terminal-dat "${INSTALLED_NAME}=${VTEBENCH_RESULTS_DIR}/installed.dat" \
        --terminal-dat "${LOCAL_NAME}=${VTEBENCH_RESULTS_DIR}/local.dat" \
        --output "${VTEBENCH_REPORT}"
fi

if [[ -n "${KITTEN_REPORT:-}" ]]; then
    printf '\n=== Kitty benchmark ===\n'
    cat "${KITTEN_REPORT}"
fi
if [[ -n "${VTEBENCH_REPORT:-}" ]]; then
    printf '\n=== vtebench ===\n'
    cat "${VTEBENCH_REPORT}"
fi

printf '\nArtifacts: %s\n' "${RESULTS_DIR}"
