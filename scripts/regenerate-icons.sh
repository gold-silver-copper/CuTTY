#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_png="$repo_root/CuTTY2.png"
canonical_png="$repo_root/extra/logo/cutty-term.png"
compat_png="$repo_root/extra/logo/compat/cutty-term.png"
promo_png="$repo_root/extra/promo/cutty-readme.png"
windows_ico="$repo_root/cutty/windows/cutty.ico"
macos_icns="$repo_root/extra/osx/CuTTY.app/Contents/Resources/cutty.icns"

if ! command -v python3 >/dev/null 2>&1; then
    echo "error: python3 is required" >&2
    exit 1
fi

if ! python3 -c "import PIL" >/dev/null 2>&1; then
    echo "error: Pillow is required (python3 -m pip install Pillow)" >&2
    exit 1
fi

cp "$source_png" "$canonical_png"
cp "$source_png" "$compat_png"
cp "$source_png" "$promo_png"

SOURCE_PNG="$source_png" WINDOWS_ICO="$windows_ico" MACOS_ICNS="$macos_icns" python3 - <<'PY'
import os

from PIL import Image

source_png = os.environ["SOURCE_PNG"]
windows_ico = os.environ["WINDOWS_ICO"]
macos_icns = os.environ["MACOS_ICNS"]

source = Image.open(source_png).convert("RGBA")

master = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
fit = source.copy()
fit.thumbnail((1024, 1024), Image.Resampling.NEAREST)
x = (master.width - fit.width) // 2
y = (master.height - fit.height) // 2
master.alpha_composite(fit, (x, y))

master.save(
    windows_ico,
    format="ICO",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
master.save(
    macos_icns,
    format="ICNS",
    sizes=[(16, 16), (32, 32), (64, 64), (128, 128), (256, 256), (512, 512), (1024, 1024)],
)
PY
