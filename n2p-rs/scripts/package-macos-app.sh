#!/usr/bin/env bash
# 產生 dist/NBLM2PPTX.app（只包圖形介面 n2p-gui，不含命令列 n2p）。
#
#   ./scripts/package-macos-app.sh                  # ad-hoc 簽章，本機使用
#   ./scripts/package-macos-app.sh --developer-id   # Developer ID + Hardened Runtime
#   ./scripts/notarize-macos.sh                     # 接著公證並釘上票證
#   ./scripts/package-macos-app.sh --app-store      # Mac App Store：沙盒 + profile → dist/NBLM2PPTX.pkg
#   ./scripts/package-macos-app.sh --sandbox-test   # 沙盒版本機實測（開發憑證，不需 profile，不產 pkg）
#
# 模型不包進 app：第一次使用時由介面下載到 ~/Library/Caches/n2p/（沙盒版落在容器內）。
# 只支援 Apple Silicon。App Store 流程見 docs/APP_STORE.md。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

APP_NAME="NBLM2PPTX"
BUNDLE_ID="com.yikai.nblm2pptx"
DIST="$ROOT/dist"
APP="$DIST/$APP_NAME.app"
PKG="$DIST/$APP_NAME.pkg"
DEFAULT_IDENTITY="Developer ID Application: Henyi Lai (S6EDV86VSB)"
APPSTORE_APP_IDENTITY="${APPSTORE_APP_IDENTITY:-3rd Party Mac Developer Application: Henyi Lai (S6EDV86VSB)}"
APPSTORE_INSTALLER_IDENTITY="${APPSTORE_INSTALLER_IDENTITY:-3rd Party Mac Developer Installer: Henyi Lai (S6EDV86VSB)}"
SANDBOX_TEST_IDENTITY="${SANDBOX_TEST_IDENTITY:-Apple Development: Henyi Lai (SESYZEB89N)}"
APPSTORE_ENTITLEMENTS="$ROOT/macos/n2p-appstore.entitlements"
APPSTORE_PROFILE="$ROOT/macos/n2p-appstore.provisionprofile"

SKIP_BUILD=0
ZIP=0
DEVELOPER_ID=0
APP_STORE=0
SANDBOX_TEST=0

usage() {
  cat <<EOF
Usage: package-macos-app.sh [--skip-build] [--zip] [--developer-id | --app-store | --sandbox-test]

  --skip-build    沿用已編譯的 n2p-gui，不重新編譯
  --zip           另外寫出 dist/$APP_NAME.app.zip
  --developer-id  用 CODESIGN_IDENTITY（預設 ${DEFAULT_IDENTITY}）
                  加 Hardened Runtime 與時間戳簽章；不公證
  --app-store     cargo feature appstore、App Sandbox、嵌入
                  macos/n2p-appstore.provisionprofile，以 APPSTORE_APP_IDENTITY 簽 app、
                  APPSTORE_INSTALLER_IDENTITY 簽 dist/$APP_NAME.pkg；
                  CFBundleVersion 取自 macos/appstore-build-number
  --sandbox-test  沙盒行為同 --app-store，但編譯 feature sandbox-test（保留 N2P_*
                  除錯掛鉤以便自動化實測），改用 SANDBOX_TEST_IDENTITY 簽章、去掉需要
                  profile 的 application-identifier／team-identifier；只能本機 open，不能上傳
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1 ;;
    --zip) ZIP=1 ;;
    --developer-id) DEVELOPER_ID=1 ;;
    --app-store) APP_STORE=1 ;;
    --sandbox-test) SANDBOX_TEST=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

if (( DEVELOPER_ID + APP_STORE + SANDBOX_TEST > 1 )); then
  echo "--developer-id、--app-store、--sandbox-test 是不同產物，只能選一個" >&2
  exit 2
fi
SANDBOXED=$(( APP_STORE + SANDBOX_TEST ))

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "需要在 Apple Silicon 的 macOS 上打包" >&2
  exit 1
fi

check_identity() { # $1 = 身分名稱；$2 = 額外的 find-identity 參數（可省略）
  security find-identity -v ${2:-} | grep -qF "$1" ||
    { echo "keychain 找不到簽章身分：$1" >&2; exit 1; }
}

if [[ "$APP_STORE" -eq 1 ]]; then
  [[ -f "$APPSTORE_PROFILE" ]] || {
    echo "缺少 ${APPSTORE_PROFILE}（$BUNDLE_ID 的 Mac App Store provisioning profile，見 docs/APP_STORE.md）" >&2
    exit 1
  }
  check_identity "$APPSTORE_APP_IDENTITY" "-p codesigning"
  check_identity "$APPSTORE_INSTALLER_IDENTITY"
  command -v productbuild >/dev/null || { echo "找不到 productbuild（需要 Xcode）" >&2; exit 1; }
fi
if [[ "$SANDBOX_TEST" -eq 1 ]]; then
  check_identity "$SANDBOX_TEST_IDENTITY" "-p codesigning"
fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
if [[ "$SANDBOXED" -gt 0 ]]; then
  # App Store Connect 要求每次上傳的 CFBundleVersion 都比上一次大
  BUILD="$(tr -d '[:space:]' < macos/appstore-build-number)"
  [[ "$BUILD" =~ ^[0-9]+$ ]] || { echo "macos/appstore-build-number 必須是整數" >&2; exit 1; }
  # 分開輸出目錄：appstore／sandbox-test 的二進位不能混進其他產物
  if [[ "$APP_STORE" -eq 1 ]]; then
    TARGET_DIR="$ROOT/target/appstore"
    FEATURES=(--features appstore)
  else
    TARGET_DIR="$ROOT/target/sandbox-test"
    FEATURES=(--features sandbox-test)
  fi
else
  BUILD="$VERSION"
  TARGET_DIR="$ROOT/target"
  FEATURES=()
fi
BIN="$TARGET_DIR/release/n2p-gui"
if [[ "$SKIP_BUILD" -eq 0 ]]; then
  cargo build --release -p n2p-gui --target-dir "$TARGET_DIR" ${FEATURES[@]+"${FEATURES[@]}"}
fi
[[ -x "$BIN" ]] || { echo "找不到 $BIN" >&2; exit 1; }

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
# 先 strip 再簽章：簽章後再改動執行檔會使簽章失效。
# 執行檔改名為 APP_NAME：winit 的應用程式選單（About／Hide／Quit）用的是行程名稱，不是 CFBundleName
EXE="$APP/Contents/MacOS/$APP_NAME"
strip -x -o "$EXE" "$BIN"
sed -e "s/__VERSION__/$VERSION/g" -e "s/__BUILD__/$BUILD/g" macos/Info.plist > "$APP/Contents/Info.plist"
if [[ "$SANDBOXED" -gt 0 ]]; then
  # 商店版只宣告實測過的系統：macOS 11 未實測（LaMa 的 GPU 路徑需要 12）
  /usr/libexec/PlistBuddy -c "Set :LSMinimumSystemVersion 12.0" "$APP/Contents/Info.plist"
fi
plutil -lint "$APP/Contents/Info.plist" >/dev/null
cp macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

# 送審會被拒的私有 API（winit 的視窗模糊；已由 vendor/winit 移除）
if nm -u "$EXE" | grep -E '_(CGS|SLS)' | grep -v '_CGShieldingWindowLevel'; then
  echo "執行檔引用了私有 API（見上），App Store 會拒絕" >&2
  [[ "$SANDBOXED" -eq 0 ]] || exit 1
fi

if [[ "$APP_STORE" -eq 1 ]]; then
  cp "$APPSTORE_PROFILE" "$APP/Contents/embedded.provisionprofile"
  codesign --force --timestamp --entitlements "$APPSTORE_ENTITLEMENTS" \
    --sign "$APPSTORE_APP_IDENTITY" "$APP"
elif [[ "$SANDBOX_TEST" -eq 1 ]]; then
  TEST_ENTITLEMENTS="$(mktemp -t n2p-sandbox-test)"
  cp "$APPSTORE_ENTITLEMENTS" "$TEST_ENTITLEMENTS"
  /usr/libexec/PlistBuddy -c "Delete :com.apple.application-identifier" "$TEST_ENTITLEMENTS"
  /usr/libexec/PlistBuddy -c "Delete :com.apple.developer.team-identifier" "$TEST_ENTITLEMENTS"
  codesign --force --entitlements "$TEST_ENTITLEMENTS" --sign "$SANDBOX_TEST_IDENTITY" "$APP"
  rm -f "$TEST_ENTITLEMENTS"
elif [[ "$DEVELOPER_ID" -eq 1 ]]; then
  IDENTITY="${CODESIGN_IDENTITY:-$DEFAULT_IDENTITY}"
  check_identity "$IDENTITY" "-p codesigning"
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$APP"
  echo "Developer ID 簽章完成（${IDENTITY}）。下一步：./scripts/notarize-macos.sh"
else
  codesign --force --sign - "$APP"
  echo "ad-hoc 簽章完成（只適合本機使用）"
fi
codesign --verify --strict --verbose=2 "$APP"

if [[ "$SANDBOXED" -gt 0 ]]; then
  codesign -d --entitlements :- "$APP" 2>/dev/null | grep -q 'com.apple.security.app-sandbox' ||
    { echo "簽好的 app 沒有 App Sandbox entitlement" >&2; exit 1; }
  echo "沙盒簽章完成（build ${BUILD}）"
fi

if [[ "$APP_STORE" -eq 1 ]]; then
  rm -f "$PKG"
  productbuild --component "$APP" /Applications --sign "$APPSTORE_INSTALLER_IDENTITY" "$PKG"
  pkgutil --check-signature "$PKG" | head -3
  echo "寫出 ${PKG}（上傳：fastlane mac beta 或 fastlane mac release，見 docs/APP_STORE.md）"
fi

if [[ "$ZIP" -eq 1 ]]; then
  rm -f "$DIST/$APP_NAME.app.zip"
  ditto -c -k --keepParent "$APP" "$DIST/$APP_NAME.app.zip"
  echo "寫出 $DIST/$APP_NAME.app.zip"
fi

echo "${APP}（${BUNDLE_ID}，版本 ${VERSION}，build ${BUILD}）"
