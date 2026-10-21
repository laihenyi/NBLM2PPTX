#!/usr/bin/env bash
# 公證已用 Developer ID 簽章的 dist/NotebookLM2PPT.app，釘上票證，寫出可發布的 zip。
#
#   ./scripts/package-macos-app.sh --developer-id
#   ./scripts/notarize-macos.sh
#
# 憑證只從環境變數讀，不要寫進 repo，也不要把 .p8 放進 repo：
#   APP_STORE_CONNECT_API_KEY_PATH   AuthKey_<KEY_ID>.p8 的路徑
#   APP_STORE_CONNECT_KEY_ID         Key ID
#   APP_STORE_CONNECT_ISSUER_ID      Issuer UUID
# 或用 `xcrun notarytool store-credentials` 建立的 keychain profile：
#   NOTARYTOOL_PROFILE
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/dist/NotebookLM2PPT.app}"
OUT_ZIP="$ROOT/dist/NotebookLM2PPT-notarized.zip"

fail() { echo "error: $*" >&2; exit 1; }

[[ -d "$APP" && -f "$APP/Contents/Info.plist" ]] ||
  fail "找不到 ${APP}，先執行 ./scripts/package-macos-app.sh --developer-id"
SIGNATURE="$(codesign -dvv "$APP" 2>&1)"
[[ "$SIGNATURE" == *"Authority=Developer ID Application"* ]] ||
  fail "$APP 不是 Developer ID 簽章，先執行 ./scripts/package-macos-app.sh --developer-id"
[[ "$SIGNATURE" == *"(runtime)"* ]] ||
  fail "$APP 沒有 Hardened Runtime，公證會被拒"

NOTARY_ARGS=()
if [[ -n "${NOTARYTOOL_PROFILE:-}" ]]; then
  NOTARY_ARGS=(--keychain-profile "$NOTARYTOOL_PROFILE")
elif [[ -n "${APP_STORE_CONNECT_API_KEY_PATH:-}" && -n "${APP_STORE_CONNECT_KEY_ID:-}" && -n "${APP_STORE_CONNECT_ISSUER_ID:-}" ]]; then
  [[ -f "$APP_STORE_CONNECT_API_KEY_PATH" ]] || fail "APP_STORE_CONNECT_API_KEY_PATH 不是檔案"
  NOTARY_ARGS=(--key "$APP_STORE_CONNECT_API_KEY_PATH"
               --key-id "$APP_STORE_CONNECT_KEY_ID"
               --issuer "$APP_STORE_CONNECT_ISSUER_ID")
else
  fail "缺少公證憑證：設定 NOTARYTOOL_PROFILE，或三個 APP_STORE_CONNECT_* 變數"
fi

SUBMIT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/n2p-notary.XXXXXX")"
trap 'rm -rf "$SUBMIT_DIR"' EXIT
ditto -c -k --keepParent "$APP" "$SUBMIT_DIR/app.zip"

echo "== notarytool submit --wait =="
xcrun notarytool submit "$SUBMIT_DIR/app.zip" --wait "${NOTARY_ARGS[@]}" | tee "$SUBMIT_DIR/result.txt"
grep -q "status: Accepted" "$SUBMIT_DIR/result.txt" ||
  fail "公證未通過。用上面的 submission id 查原因：xcrun notarytool log <id> 加同樣的憑證參數"

echo "== staple =="
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"

echo "== Gatekeeper =="
spctl --assess --type execute -vv "$APP"

rm -f "$OUT_ZIP"
ditto -c -k --keepParent "$APP" "$OUT_ZIP"
echo "可發布：$OUT_ZIP"
