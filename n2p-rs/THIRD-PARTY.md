# 第三方來源與授權

n2p-rs 本身採 MIT 授權（見 repo 根目錄 `LICENSE`）。以下內容來自第三方。

## NotebookLM2PPT（上游 Python 版）

- 來源：<https://github.com/elliottzheng/NotebookLM2PPT>
- 授權：MIT，Copyright (c) 2026 Elliott Zheng（全文附於下方）
- n2p-rs 是它的 Rust 改寫。下列模組移植自上游的 Python 實作，原始檔名寫在各檔的模組註解：
  `n2p-image`（`diversity`、`inpaint`、`transform`、`watermark`）、`n2p-pdf`、`n2p-mineru`、
  `n2p-build` 的部分版面邏輯。
- `fixtures/floyd.pdf`：上游 `examples/floyd.pdf`，用於點陣化幾何與管線測試。
- `fixtures/image-fixtures/`：用上游的 Python 實作算出的參考輸出（產生方式見
  `fixtures/make_image_fixtures.py`）。

## winit（vendored）

- 來源：<https://github.com/rust-windowing/winit> 0.30.13，授權 Apache-2.0（`vendor/winit/LICENSE`）
- `vendor/winit` 是它的修改版，經 `Cargo.toml` 的 `[patch.crates-io]` 取代 crates.io 版本。
  唯一的修改：移除私有 API `CGSSetWindowBackgroundBlurRadius` 的宣告與呼叫（Mac App Store 審核會拒絕），
  修改處以 `n2p:` 註解標示（`src/platform_impl/macos/ffi.rs`、`window_delegate.rs`）。

## 執行期下載的模型（不隨原始碼或 app 散布）

| 模型 | 下載來源 | 授權 |
| :--- | :--- | :--- |
| LaMa（`lama_fp32.onnx`） | <https://huggingface.co/Carve/LaMa-ONNX> | Apache-2.0 |
| MobileSAM encoder／decoder | <https://huggingface.co/Acly/MobileSAM> | ONNX 匯出為 MIT；原始 MobileSAM 為 Apache-2.0 |

模型在第一次使用時下載到 `~/Library/Caches/n2p/`。LaMa 的 GPU 版
（`lama_fp32.coreml-v1.onnx`）是在使用者機器上由原模型改寫而成，同樣不散布。

## ONNX protobuf 定義

- `crates/n2p-image/proto/onnx.proto`：ONNX 官方定義，Apache-2.0（檔頭有 SPDX 標示）。
- `crates/n2p-image/src/onnx_pb/onnx.rs`：由上述檔案以 rust-protobuf 3.7.2 產生。

## 附錄：NotebookLM2PPT 授權全文

```text
MIT License

Copyright (c) 2026 Elliott Zheng

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
