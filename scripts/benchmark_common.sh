#!/usr/bin/env bash

set -euo pipefail

benchmark_fail() {
    echo "error: $*" >&2
    exit 1
}

benchmark_status() {
    echo "[$(date +"%Y-%m-%d %H:%M:%S")] $*"
}

benchmark_require_dir() {
    local path="$1"
    local description="$2"
    [[ -d "${path}" ]] || benchmark_fail "${description} not found: ${path}"
}

benchmark_require_file() {
    local path="$1"
    local description="$2"
    [[ -f "${path}" ]] || benchmark_fail "${description} not found: ${path}"
}

benchmark_resolve_binary() {
    local explicit="${1:-}"
    local fallback_path="${2:-}"
    local command_name="${3:-}"

    if [[ -n "${explicit}" ]]; then
        [[ -x "${explicit}" ]] || benchmark_fail "binary is not executable: ${explicit}"
        printf '%s\n' "${explicit}"
        return 0
    fi

    if [[ -n "${fallback_path}" && -x "${fallback_path}" ]]; then
        printf '%s\n' "${fallback_path}"
        return 0
    fi

    if [[ -n "${command_name}" ]]; then
        if command -v "${command_name}" >/dev/null 2>&1; then
            command -v "${command_name}"
            return 0
        fi
    fi

    benchmark_fail "unable to resolve binary (explicit='${explicit}', fallback='${fallback_path}', command='${command_name}')"
}

benchmark_app_bundle_from_path() {
    local path="$1"
    if [[ "${path}" == *.app ]]; then
        printf '%s\n' "${path}"
        return 0
    fi

    if [[ "${path}" == *.app/* ]]; then
        local app_path="${path%%.app/*}.app"
        printf '%s\n' "${app_path}"
        return 0
    fi

    return 1
}

benchmark_launch_terminal() {
    local terminal_kind="$1"
    local terminal_name="$2"
    local terminal_bin="$3"
    shift 3

    benchmark_status "Launching ${terminal_name}: ${terminal_bin}"
    case "${terminal_kind}" in
        cutty|alacritty)
            "${terminal_bin}" -e "$@" >/dev/null 2>&1 &
            ;;
        kitty)
            "${terminal_bin}" "$@" >/dev/null 2>&1 &
            ;;
        ghostty)
            if [[ "${OSTYPE:-}" == darwin* ]]; then
                local app_bundle
                app_bundle="$(benchmark_app_bundle_from_path "${terminal_bin}")" || \
                    benchmark_fail "unable to derive Ghostty app bundle from ${terminal_bin}"
                open -na "${app_bundle}" --args -e "$@" >/dev/null 2>&1 &
            else
                "${terminal_bin}" -e "$@" >/dev/null 2>&1 &
            fi
            ;;
        *)
            benchmark_fail "unsupported terminal kind: ${terminal_kind}"
            ;;
    esac
}

benchmark_wait_for_markers() {
    local results_dir="$1"
    local timeout_seconds="$2"
    shift 2

    local labels=("$@")
    local start_ts
    start_ts="$(date +%s)"
    local last_reported=0

    while true; do
        local all_done=1
        local label
        for label in "${labels[@]}"; do
            if [[ ! -f "${results_dir}/${label}.done" ]]; then
                all_done=0
                break
            fi
        done

        if [[ "${all_done}" -eq 1 ]]; then
            return 0
        fi

        local now
        now="$(date +%s)"
        local elapsed=$((now - start_ts))
        if (( now - start_ts >= timeout_seconds )); then
            benchmark_fail "timed out waiting for benchmark completion markers in ${results_dir}"
        fi

        if (( elapsed >= 15 && elapsed - last_reported >= 15 )); then
            benchmark_status "Still waiting on: ${labels[*]} (${elapsed}s elapsed)"
            last_reported="${elapsed}"
        fi

        sleep 1
    done
}

benchmark_check_status_files() {
    local results_dir="$1"
    shift

    local labels=("$@")
    local failed=0
    local label
    for label in "${labels[@]}"; do
        local status_file="${results_dir}/${label}.status"
        if [[ ! -f "${status_file}" ]]; then
            echo "Missing status file: ${status_file}" >&2
            failed=1
            continue
        fi

        local status
        status="$(<"${status_file}")"
        if [[ "${status}" != "0" ]]; then
            echo "Benchmark failed for ${label} with exit status ${status}" >&2
            failed=1
        fi
    done

    return "${failed}"
}
