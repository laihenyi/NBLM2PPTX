# NBLM2PPTX - NotebookLM PDF 轉 PPTX 工具

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

將 NotebookLM 產出的 PDF 轉換成**底圖與文字分離**的 PPTX 簡報檔案。

> ✨ **更新 (2026-10-21)**: macOS 原生版 NBLM2PPTX v0.1.1 發布——在 Mac 本機把 PDF 轉成可編輯 PPTX，不需要 API key，已通過 Apple 公證。詳見下方「macOS 原生版」。

[English](README.md) | [简体中文](README-zh-CN.md) | [日本語](README-ja.md) | [Español](README-es.md) | [Français](README-fr.md)

## macOS 原生版（離線，Rust）

**NBLM2PPTX for macOS** 是在 Mac 本機執行的原生 App：不需要 API key，檔案不上傳。原始碼在 [`macos-app/`](macos-app/)。

- **下載**：[Releases 頁面](https://github.com/laihenyi/NBLM2PPTX/releases/tag/n2p-rs-v0.1.1)（`NotebookLM2PPT-notarized.zip`）
- **OCR**：macOS 內建 Vision（裝置端）
- **去字**：只擦文字筆畫；有紋理的背景由 LaMa 在 GPU（CoreML）上重建
- **物件切出（實驗性）**：圖示、插圖變成可移動的圖片（MobileSAM）
- **App**：拖放式圖形介面，Developer ID 簽章並通過 Apple 公證；模型（約 253 MB）在第一次開啟時下載
- **需求**：Apple Silicon；GPU 推論需 macOS 12 以上

建置、命令列用法與設計筆記見 [`macos-app/README.md`](macos-app/README.md)；第三方授權見 [`macos-app/THIRD-PARTY.md`](macos-app/THIRD-PARTY.md)。

### 前後比較（標準版）

| 原稿（NotebookLM PDF） | 輸出（可編輯 PPTX，macOS App） |
|:---:|:---:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-native-v0.1.1-alpha6.jpg" width="400"> |
| <img src="assets/demo-v1.1-original.jpg" width="400"> | <img src="assets/demo-native-v0.1.1-kitchen.jpg" width="400"> |

> 文字變成可編輯的文字框；插圖留在背景，NotebookLM 浮水印已移除（輸出以 LibreOffice 渲染）。
> 圖中可見的現有限制：螢光標記色不保留、粗體偶爾遺失、OCR 偶有錯字。

### v0.1.1（2026-10-21）

- **物件版快約一倍，記憶體峰值從約 6 GB 降到約 1.9 GB**：SAM 改在純 CPU 上跑（11 頁簡報 47–86 秒，原本 103–183 秒）
- 多行內文依原稿靠左（原本一律置中）
- 物件切出的遮罩外擴加速
- GUI：模型區塊顯示 LaMa 的 GPU 版是否就緒
- 開發工具：PowerPoint 相容性檢查只讀取、只關閉自己開啟的簡報

### v0.1.0（2026-10-21）

- 首次發布：拖放式 App，已簽章並通過公證
- 速度（Apple Silicon，11 頁簡報）：標準版 29–45 秒（約 1.1–1.4 GB），物件版 103–183 秒（約 6 GB）

## Gemini 輔助網頁版（已停止更新）

早期的 Gemini 輔助網頁版（`index.html` 與各語言版本、示範影片專案）已封存在 [`legacy-web/`](legacy-web/)，**不再更新**；需要的開發者可自行取用。說明文件：[`legacy-web/README-zh-TW.md`](legacy-web/README-zh-TW.md)。

## License

MIT License
