#!/usr/bin/env bash
# 由 macos/AppIcon.svg 重新產生 macos/AppIcon.icns（含 App Store 需要的 1024×1024）。
#
#   ./scripts/make-icon.sh
#
# 需要 Google Chrome（headless 繪製 SVG，濾鏡與字型才正確）與 ImageMagick（縮圖）。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
[[ -x "$CHROME" ]] || { echo "找不到 Chrome：${CHROME}（可用 CHROME 指定）" >&2; exit 1; }
command -v magick >/dev/null || { echo "需要 ImageMagick（magick）" >&2; exit 1; }

WORK="$(mktemp -d -t n2p-icon)"
trap 'rm -rf "$WORK"' EXIT
MASTER="$WORK/master.png"
"$CHROME" --headless --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
  --default-background-color=00000000 --window-size=1024,1024 \
  --screenshot="$MASTER" "file://$ROOT/macos/AppIcon.svg" 2>/dev/null
[[ "$(magick identify -format '%wx%h' "$MASTER")" == "1024x1024" ]] ||
  { echo "SVG 繪製失敗" >&2; exit 1; }

SET="$WORK/AppIcon.iconset"
mkdir "$SET"
for size in 16 32 128 256 512; do
  magick "$MASTER" -filter Lanczos -resize "${size}x${size}" "$SET/icon_${size}x${size}.png"
  double=$((size * 2))
  if [[ "$double" -eq 1024 ]]; then
    cp "$MASTER" "$SET/icon_${size}x${size}@2x.png"
  else
    magick "$MASTER" -filter Lanczos -resize "${double}x${double}" "$SET/icon_${size}x${size}@2x.png"
  fi
done
iconutil -c icns "$SET" -o "$ROOT/macos/AppIcon.icns"
echo "寫出 macos/AppIcon.icns"
