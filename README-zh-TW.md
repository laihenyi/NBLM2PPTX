# NBLM2PPTX - NotebookLM PDF 轉 PPTX 工具

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

將 NotebookLM 產出的 PDF 轉換成**底圖與文字分離**的 PPTX 簡報檔案。

[English](README.md) | [简体中文](README-zh-CN.md) | [日本語](README-ja.md) | [Español](README-es.md) | [Français](README-fr.md)

## 演示

### v1.1 - 混合文字提取

| 原始 (NotebookLM PDF) | 輸出 (可編輯 PPTX) |
|:---------------------:|:------------------:|
| <img src="assets/demo-v1.1-original.jpg" width="350"> | <img src="assets/demo-v1.1-output.jpg" width="350"> |

> 轉換後的 PPTX 已移除浮水印，並保留可編輯的文字圖層。

### v1.0 - AI 智能去字

| 轉換前 (NotebookLM PDF) | 轉換後 (可編輯 PPTX) |
|:-----------------------:|:--------------------:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-before.png" width="400"> |

> 左：NotebookLM 原始 PDF（文字嵌入圖片中）
> 右：轉換後的 PPTX，乾淨背景 + 可編輯文字圖層

## v2.0 新功能

### 重要提示 (v2.0.1)

由於圖形辨識 AI 決定了輸出品質，**不建議一次轉換多頁**。建議**每次處理 5 張以內**，以獲得最佳轉換效果。

### 平行處理

文字移除與 OCR 辨識現在同時執行，大幅縮短圖片來源的處理時間。

### 批次處理與速率保護

每 3 頁為一批次，批次間有 6 秒冷卻時間，降低處理大量頁面時的 API 429 錯誤。

### PDF OCR 回退機制

自動偵測無文字層的 PDF（如 NotebookLM 匯出檔案），自動回退使用 OCR，確保所有 PDF 類型都能產生文字圖層。

### 穩定性提升

- 改進文字移除，更好地保留非文字元素
- 新增驗證機制，跳過無效的 OCR 結果
- 強化邊緣案例的錯誤處理

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
