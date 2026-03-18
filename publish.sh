#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${ROOT_DIR}"

DRY_RUN=0
ALLOW_DIRTY=0
WAIT_SECONDS=30

usage() {
    cat <<'EOF'
Usage: ./publish.sh [--dry-run] [--allow-dirty] [--wait-seconds N]

Publishes the CuTTY workspace crates to crates.io in dependency order:
  1. cutty_config
  2. cutty_config_derive
  3. cutty_terminal
  4. cutty

Options:
  --dry-run         Run `cargo publish --dry-run` for each crate.
  --allow-dirty     Pass `--allow-dirty` to cargo publish.
  --wait-seconds N  Seconds to wait between publishes. Default: 30.
  --help            Show this help.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --allow-dirty)
            ALLOW_DIRTY=1
            shift
            ;;
        --wait-seconds)
            WAIT_SECONDS="${2:?missing value for --wait-seconds}"
            shift 2
            ;;
        --help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

CRATES=(
    cutty_config
    cutty_config_derive
    cutty_terminal
    cutty
)

publish_one() {
    local crate="$1"
    local -a cmd=(cargo publish -p "${crate}")

    if [[ "${DRY_RUN}" -eq 1 ]]; then
        cmd+=(--dry-run)
    fi

    if [[ "${ALLOW_DIRTY}" -eq 1 ]]; then
        cmd+=(--allow-dirty)
    fi

    echo
    echo "==> ${cmd[*]}"
    "${cmd[@]}"
}

echo "Publishing from ${ROOT_DIR}"
echo "Crates: ${CRATES[*]}"
if [[ "${DRY_RUN}" -eq 1 ]]; then
    echo "Mode: dry-run"
else
    echo "Mode: publish"
fi

for i in "${!CRATES[@]}"; do
    crate="${CRATES[$i]}"
    publish_one "${crate}"

    if [[ "${DRY_RUN}" -eq 0 && "$i" -lt $((${#CRATES[@]} - 1)) ]]; then
        next_crate="${CRATES[$((i + 1))]}"
        echo
        echo "Waiting ${WAIT_SECONDS}s before publishing ${next_crate}..."
        sleep "${WAIT_SECONDS}"
    fi
done

echo
echo "Done."
