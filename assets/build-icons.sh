#!/usr/bin/env bash
#
# Build all platform icons from assets/icon.svg.
# Outputs into:
#   - src-tauri/icons/  (Tauri bundle: PNGs, .icns, .ico, Square*Logo.png)
#   - public/           (PWA manifest icons)
#
# Run from repo root: ./assets/build-icons.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SVG="$ROOT/assets/icon.svg"
TAURI="$ROOT/src-tauri/icons"
PUBLIC="$ROOT/public"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if ! command -v rsvg-convert >/dev/null; then
  echo "rsvg-convert not found (brew install librsvg)" >&2
  exit 1
fi

mkdir -p "$TAURI" "$PUBLIC"

render() {
  local size=$1 out=$2
  rsvg-convert -w "$size" -h "$size" "$SVG" -o "$out"
}

echo "→ Tauri PNGs"
render 32   "$TAURI/32x32.png"
render 128  "$TAURI/128x128.png"
render 256  "$TAURI/128x128@2x.png"
render 1024 "$TAURI/icon.png"

echo "→ Windows tile PNGs"
render 30  "$TAURI/Square30x30Logo.png"
render 44  "$TAURI/Square44x44Logo.png"
render 71  "$TAURI/Square71x71Logo.png"
render 89  "$TAURI/Square89x89Logo.png"
render 107 "$TAURI/Square107x107Logo.png"
render 142 "$TAURI/Square142x142Logo.png"
render 150 "$TAURI/Square150x150Logo.png"
render 284 "$TAURI/Square284x284Logo.png"
render 310 "$TAURI/Square310x310Logo.png"
render 50  "$TAURI/StoreLogo.png"

echo "→ macOS .icns"
ICONSET="$TMP/icon.iconset"
mkdir -p "$ICONSET"
render 16   "$ICONSET/icon_16x16.png"
render 32   "$ICONSET/icon_16x16@2x.png"
render 32   "$ICONSET/icon_32x32.png"
render 64   "$ICONSET/icon_32x32@2x.png"
render 128  "$ICONSET/icon_128x128.png"
render 256  "$ICONSET/icon_128x128@2x.png"
render 256  "$ICONSET/icon_256x256.png"
render 512  "$ICONSET/icon_256x256@2x.png"
render 512  "$ICONSET/icon_512x512.png"
render 1024 "$ICONSET/icon_512x512@2x.png"
iconutil -c icns "$ICONSET" -o "$TAURI/icon.icns"

echo "→ Windows multi-res .ico"
ICO_BASE="$TMP/ico-256.png"
render 256 "$ICO_BASE"
python3 - "$TAURI/icon.ico" "$ICO_BASE" <<'PY'
import sys
from PIL import Image
out_path, src = sys.argv[1], sys.argv[2]
# Open the largest source PNG and let Pillow downscale into a multi-resolution
# .ico in one call. This is the only invocation that reliably writes all sizes.
img = Image.open(src).convert("RGBA")
img.save(
    out_path,
    format="ICO",
    sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
PY

echo "→ PWA icons"
render 128 "$PUBLIC/icon-128.png"
render 256 "$PUBLIC/icon-256.png"
render 512 "$PUBLIC/icon-512.png"

echo
echo "Done. Outputs:"
ls -la "$TAURI"
echo
ls -la "$PUBLIC"
