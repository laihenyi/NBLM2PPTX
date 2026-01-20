# NBLM2PPTX - NotebookLM PDF 轉 PPTX 工具

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

將 NotebookLM 產出的 PDF 轉換成**底圖與文字分離**的 PPTX 簡報檔案。

> ✨ **更新 (2026-01-21)**: v2.2.1 版本發布 - 完整多語系改版！所有語言版本現採用專業淺色主題設計，改善使用者體驗並標準化文件。

[English](README.md) | [简体中文](README-zh-CN.md) | [日本語](README-ja.md) | [Español](README-es.md) | [Français](README-fr.md)

## 演示

### v1.1 - 混合文字提取

| 原始 (NotebookLM PDF) | 輸出 (可編輯 PPTX) |
|:---------------------:|:------------------:|
| <img src="assets/demo-v1.1-original.jpg" width="400"> | <img src="assets/demo-v1.1-output.jpg" width="400"> |

> PDF.js 原生文字提取提供精確的文字定位，無需額外 API 調用。

### v1.0 - AI 智能去字

| 轉換前 (NotebookLM PDF) | 轉換後 (可編輯 PPTX) |
|:-----------------------:|:--------------------:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-before.png" width="400"> |

> 左：NotebookLM 原始 PDF（文字嵌入圖片中）
> 右：轉換後的 PPTX，乾淨背景 + 可編輯文字圖層

## v2.3 版本更新 (2026-01-21)

### ⚡ 雙模式 OCR 系統
- **Lite 模型（預設）**：使用 `gemini-2.5-flash-lite` 進行 OCR，速度快 40-50%，節省 50% API 額度
- **標準模型（可選）**：使用 `gemini-2.5-flash` 完整檢測字體大小、粗細、顏色等樣式
- **使用者彈性選擇**：在頁面選擇階段可自由切換 OCR 模型，根據需求平衡速度與品質

### 🚀 並行處理技術
- **同步執行**：文字移除與 OCR 同時進行，無需等待
- **處理時間縮短**：從單頁 3-4 秒降至 2-3 秒
- **智能容錯**：單一 API 失敗不影響整體流程，提升穩定性

### 💡 清晰的使用建議
- **Lite 模型適合**：純文字筆記、會議記錄、內容草稿（不在意視覺格式）
- **標準模型適合**：精美簡報、品牌展示、教學投影片（需保留視覺層次）
- **透明化限制說明**：明確告知 Lite 模型無法檢測字體樣式的限制

### 📊 輸出結果對比

| Lite 模型 | 標準模型 |
|:--------:|:--------:|
| <img src="assets/demo-v2.3-lite.jpg" width="400"> | <img src="assets/demo-v2.3-standard.jpg" width="400"> |

> **Lite 模型**：所有文字採用統一樣式，無字體大小差異（速度快，省 API 額度）
> **標準模型**：完整保留標題與內文的字體大小層次（完整樣式檢測）

---

## v2.2.1 版本更新 (2026-01-21)

### 🌍 完整多語系改版
- **所有語言專業設計**：完全重新設計所有語言版本（英文、西班牙文、日文、法文、簡體中文），從深色主題升級為現代淺色主題
- **統一字體系統**：遷移至 Poppins（標題）+ Open Sans（內文），配合語言特定後備字體（Noto Sans JP、Noto Sans SC 等）
- **專業藍色配色方案**：在所有版本中實施一致的 #3B82F6 主色調，營造信任與專業感
- **增強的 API Key 模態框**：基於瀏覽器的 API Key 儲存，整合 localStorage，無需編輯程式碼
- **可摺疊 UI 元素**：新增可摺疊的提醒橫幅和工具區塊，介面更簡潔

### 📚 標準化文件
- **完整的 README**：所有語言的 README 檔案現採用 204 行完整結構
- **快速入門指南**：新增 3 步快速入門說明，優化新手體驗
- **免費 API 配額詳情**：清楚說明 Google Gemini 免費層級（15 RPM, 1500 RPD, 無需信用卡）
- **完整 FAQ 章節**：5 組問答涵蓋 API Key、安全性、失敗處理、分享和離線使用等常見問題

### 🎨 設計系統更新
- **淺色主題**：#F8FAFC 背景提升可讀性
- **現代卡片佈局**：簡潔邊框（#E2E8F0）和細微陰影
- **專業 SVG 圖示**：用專業 SVG 圖形替換 emoji 圖示
- **響應式排版**：優化所有螢幕尺寸的字體大小和間距

---

## v2.2 版本更新 (2026-01-20)

### 🎯 軟重置保留 API Key
- **無需重複輸入**：點擊「重新開始」時，API Key 會保留在記憶體中
- **無限次重新開始**：可處理多批檔案而無需重新輸入憑證
- **智能狀態管理**：重置所有處理狀態但保留您的 API Key

### ⚡ 速度優化
- **處理速度提升 70%**：頁面間延遲從 3.5 秒降至 1.0 秒
- **並行處理**：充分利用並發 API 呼叫達到最高效率
- **即時重置**：軟重置立即回到初始狀態，無需重新載入頁面

### 🔧 IMAGE_RECITATION 錯誤修復
- **改進 AI 提示詞**：優化提示工程以避免版權檢測
- **更佳背景重建**：更精確的內容感知填充結果
- **降低溫度參數**：使用溫度 0.4 獲得更一致的 AI 行為

### 📝 UI 改進
- **更清晰的說明**：更新 API Key 設定指南以符合實際流程
- **乾淨的重置介面**：重置時恢復初始上傳介面，而非載入動畫

## 功能特色

- **AI 智慧去字**：使用 Gemini 2.5 Flash 自動移除圖片中的文字並重建背景
- **混合文字提取**：PDF 來源使用 PDF.js 原生提取精確座標；圖片來源使用增強版 Gemini OCR
- **底圖文字分離**：匯出的 PPTX 中，背景圖與文字為獨立圖層，方便後續編輯
- **批次處理**：支援多頁 PDF 或多張圖片一次處理
- **頁面選擇**：可自由勾選需要處理的頁面，節省時間與 API 額度

## 使用方式

### 在 Google Gemini Canvas 中使用

1. 開啟 [Google Gemini](https://gemini.google.com/)
2. 在對話中輸入類似以下的提示：
   ```
   直接執行，不要修改
   ```
3. 當 Gemini 進入 **Canvas 模式**（畫面右側出現程式碼編輯區）
4. 將本專案的 `index.html`（或你偏好的語言版本）完整程式碼貼入 Canvas
5. 點擊 Canvas 右上角的「**Preview**」按鈕預覽執行

### API Key 設定

> **重要提示**：在 Gemini Canvas 環境中執行時，**無須設定私人 API Key**。系統會自動使用預設的 API 環境。

如果你想在 Canvas 以外的環境執行（例如自架伺服器），請在程式碼中找到以下位置，填入你的 Gemini API Key：

```javascript
const apiKey = "你的_GEMINI_API_KEY";
```

> 取得 API Key：前往 [Google AI Studio](https://aistudio.google.com/app/apikey) 建立

## 操作流程

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│  上傳 PDF   │ -> │  選擇頁面   │ -> │  AI 處理    │ -> │  匯出 PPTX  │
│  或圖片     │    │  (可多選)   │    │  去字重建   │    │  底圖+文字  │
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

### Step 1：上傳檔案
- 拖放或點擊上傳 NotebookLM 產出的 PDF
- 也支援 JPG、PNG、WebP 等圖片格式
- 可一次上傳多個檔案

> **提示**：NotebookLM 產出的 PDF 檔案尺寸可能較大，可以先使用免費 PDF 壓縮服務縮小檔案後再上傳，處理效率會更好。

### Step 2：選擇頁面
- 系統自動產生所有頁面的縮圖
- 勾選需要處理的頁面（預設全選）
- 點擊「開始處理」進入下一步

### Step 3：AI 處理
- Gemini 會逐頁移除文字並重建背景
- 處理進度即時顯示
- 每頁約需 3-5 秒（含 API 延遲）

> **注意**：Gemini 去文字可能會有不完整的情況，如發現文字殘留過多，可以重新處理一次。

### Step 4：匯出 PPTX
- 選擇簡報比例（16:9 / 9:16 / 4:3）
- 點擊「合併 PPTX」匯出
- 文字定位採用混合策略：
  - **PDF 來源**：使用 PDF.js 預先提取的座標（即時完成，無需 API）
  - **圖片來源**：使用 Gemini OCR 增強版樣式偵測

## 輸出結果

匯出的 PPTX 每頁包含：

| 圖層 | 內容 |
|------|------|
| 底層 | 去除文字後的乾淨背景圖 |
| 上層 | 可編輯的文字方塊（位置對應原始文字） |

這樣的分層結構讓你可以：
- 輕鬆修改文字內容
- 更換字型、顏色、大小
- 調整文字位置
- 保留原始設計風格

## 技術規格

| 項目 | 說明 |
|------|------|
| AI 模型 | Gemini 2.5 Flash (Image Edit + Text Gen) |
| 文字移除 | 優化提示詞實現完整去字與背景修復 |
| PDF 解析 | PDF.js 3.11.174 |
| PPTX 生成 | PptxGenJS 3.12.0 |
| 渲染解析度 | 縮圖 0.5x / 處理 2.0x |
| 支援格式 | PDF, JPG, PNG, WebP, BMP |
| 文字提取 | 混合策略：PDF.js 原生（PDF）/ Gemini OCR（圖片）|

## 注意事項

1. **API 額度**：文字移除使用 Gemini API；PDF 文字提取為本地處理（無 API 成本）
2. **Rate Limit**：若遇到 429 錯誤，系統會自動等待重試
3. **處理時間**：大量頁面建議分批處理
4. **網路需求**：需要穩定的網路連線
5. **瀏覽器**：建議使用 Chrome 或 Edge 最新版本
6. **PDF 優勢**：PDF 來源匯出更快，文字定位更精準

## 常見問題

### Q: 為什麼要用 Gemini Canvas？
A: Canvas 模式提供了一個安全的沙盒環境來執行前端程式碼，無需架設伺服器即可運行完整功能。而且會使用預設的 API 環境，無須設定私人 API Key。

### Q: 處理失敗怎麼辦？
A: 常見原因：
- API Key 無效或過期（在 Canvas 以外執行時）
- 網路連線不穩定
- 圖片過大或格式不支援
- API 呼叫頻率過高（等待後重試）

### Q: 可以離線使用嗎？
A: 不行，本工具需要呼叫 Gemini API 進行 AI 處理。

## 多語言版本

| 語言 | 檔案 |
|------|------|
| 繁體中文 | `index.html` |
| English | `index-en.html` |
| Español | `index-es.html` |
| 日本語 | `index-ja.html` |
| Français | `index-fr.html` |
| 简体中文 | `index-zh-CN.html` |

## License

MIT License
