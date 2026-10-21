#!/usr/bin/env bash
# 產生 dist/NotebookLM2PPT.app（只包圖形介面 n2p-gui，不含命令列 n2p）。
#
#   ./scripts/package-macos-app.sh                  # ad-hoc 簽章，本機使用
#   ./scripts/package-macos-app.sh --developer-id   # Developer ID + Hardened Runtime
#   ./scripts/notarize-macos.sh                     # 接著公證並釘上票證
#
# 模型不包進 app：第一次使用時由介面下載到 ~/Library/Caches/n2p/。
# 只支援 Apple Silicon。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

APP_NAME="NotebookLM2PPT"
BUNDLE_ID="com.yikai.notebooklm2ppt"
DIST="$ROOT/dist"
APP="$DIST/$APP_NAME.app"
DEFAULT_IDENTITY="Developer ID Application: Henyi Lai (S6EDV86VSB)"

SKIP_BUILD=0
ZIP=0
DEVELOPER_ID=0

usage() {
  cat <<EOF
Usage: package-macos-app.sh [--skip-build] [--zip] [--developer-id]

  --skip-build    沿用 target/release/n2p-gui，不重新編譯
  --zip           另外寫出 dist/$APP_NAME.app.zip
  --developer-id  用 CODESIGN_IDENTITY（預設 ${DEFAULT_IDENTITY}）
                  加 Hardened Runtime 與時間戳簽章；不公證
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1 ;;
    --zip) ZIP=1 ;;
    --developer-id) DEVELOPER_ID=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "需要在 Apple Silicon 的 macOS 上打包" >&2
  exit 1
fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
BIN="$ROOT/target/release/n2p-gui"
if [[ "$SKIP_BUILD" -eq 0 ]]; then
  cargo build --release -p n2p-gui
fi
[[ -x "$BIN" ]] || { echo "找不到 $BIN" >&2; exit 1; }

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
# 先 strip 再簽章：簽章後再改動執行檔會使簽章失效
strip -x -o "$APP/Contents/MacOS/n2p-gui" "$BIN"
sed "s/__VERSION__/$VERSION/g" macos/Info.plist > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null
cp macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

if [[ "$DEVELOPER_ID" -eq 1 ]]; then
  IDENTITY="${CODESIGN_IDENTITY:-$DEFAULT_IDENTITY}"
  security find-identity -v -p codesigning | grep -qF "$IDENTITY" ||
    { echo "keychain 找不到簽章身分：$IDENTITY" >&2; exit 1; }
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$APP"
  echo "Developer ID 簽章完成（${IDENTITY}）。下一步：./scripts/notarize-macos.sh"
else
  codesign --force --sign - "$APP"
  echo "ad-hoc 簽章完成（只適合本機使用）"
fi
codesign --verify --strict --verbose=2 "$APP"

if [[ "$ZIP" -eq 1 ]]; then
  rm -f "$DIST/$APP_NAME.app.zip"
  ditto -c -k --keepParent "$APP" "$DIST/$APP_NAME.app.zip"
  echo "寫出 $DIST/$APP_NAME.app.zip"
fi

echo "${APP}（${BUNDLE_ID}，版本 ${VERSION}）"
