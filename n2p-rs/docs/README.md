# Spire.Presentation 引擎解析產物

把 `spire.presentation 11.8.1` 黑箱的實際行為取出成可實作的規格，供 Rust 改寫使用。

| 文件 | 內容 |
| :--- | :--- |
| [01-spire-engine-surface.md](01-spire-engine-surface.md) | 引擎架構判定（.NET NativeAOT）、3,927 個 C ABI 匯出、本專案實際需要的 400 個、REA 使用紀錄、授權界線 |
| [02-ooxml-reference.md](02-ooxml-reference.md) | 本專案需要的 8 個操作的逐節點 OOXML 規格與單位換算 |
| [03-findings-and-rust-plan.md](03-findings-and-rust-plan.md) | 8 項關鍵發現、平台修正、各階段完成狀態與驗證結果 |
| [04-powerpoint-automation.md](04-powerpoint-automation.md) | 為何不用 PowerPoint 產生檔案、macOS AppleScript 的七個坑、驗證工具的安全規則 |

## 取得方式

```bash
python3 -m pip download spire.presentation --no-deps -d /tmp/spire_dl
unzip -q /tmp/spire_dl/spire_presentation-*.whl -d /tmp/spire_dl/x

# 匯出符號表（3,927 筆）
nm -gU "/tmp/spire_dl/x/spire/presentation/lib/Spire.Presentation.Base.dylib" \
  | awk '{print $3}' | sed 's/^_//' > research/spire-11.8.1-exports.txt

# 產生參考 PPTX
python3 -m venv /tmp/spire_venv
/tmp/spire_venv/bin/pip install /tmp/spire_dl/spire_presentation-*.whl pillow
/tmp/spire_venv/bin/python research/probe.py  <任意 png>
/tmp/spire_venv/bin/python research/probe2.py
```

## 目標平台

**macOS。** 原分析假設要移植 Windows 自動化層（pywin32／COM／模擬滑鼠），
經確認後整層刪除，改以 macOS 內建 Vision OCR 取得可編輯文字。詳見
[03-findings-and-rust-plan.md](03-findings-and-rust-plan.md#平台修正目標是-macos不是-windows)。

## 實作

這些規格已經落成程式碼：[`n2p-rs/`](../) —— 10 個 crate、123 個測試全綠。

```bash
cd n2p-rs
cargo test
cargo run -p n2p-cli -- convert deck.pdf -o deck.pptx   # Vision OCR + 去水印 + 16:9
```

## research/

- `spire-11.8.1-exports.txt` — 完整匯出符號表（3,927 行）
- `probe.py` — 複現 repo 的每一項 Spire 呼叫，輸出獨立 pptx
- `probe2.py` — 修正版：驗證 `clean_ppt` 只刪除水印、不誤刪內容
- `probe-pptx/` — 產出的參考檔案，可直接 `unzip` 讀 XML

## 三個必讀結論

1. **這個引擎是 .NET NativeAOT 原生碼，沒有 IL 可反編譯**；但它的 C ABI 匯出表與 Python 綁定層（435 個 ctypes 代理檔）就是完整規格書，且可直接執行取樣。
2. **`clean_ppt` 實際上是去水印函式**，不是清理多餘形狀。Rust 版不需要它。
3. **`AppendBySlide` 是盲目 part 匯入**，3 份同模板合併會產生 3 套 master/layout/theme。Rust 版套用同構捷徑可讓檔案小約 60%。

## 授權界線

可以：讀匯出符號、讀 Python 綁定、用試用版產檔並分析自己的輸出。
不可以：把反編譯機器碼的邏輯抄進實作。OOXML 是 ISO/IEC 29500 公開標準，照標準實作不侵犯 Spire 著作權。
