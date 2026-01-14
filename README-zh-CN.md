# NBLM2PPTX - NotebookLM PDF 转 PPTX 工具

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

将 NotebookLM 导出的 PDF 转换成**底图与文字分离**的 PPTX 演示文稿。

[English](README.md) | [繁體中文](README-zh-TW.md) | [日本語](README-ja.md) | [Español](README-es.md) | [Français](README-fr.md)

## 演示

### v2.0.2 - 水印移除

| 原始 (NotebookLM PDF) | 输出 (可编辑 PPTX) |
|:---------------------:|:------------------:|
| <img src="assets/demo-v1.1-original.jpg" width="350"> | <img src="assets/demo-v1.1-output.jpg" width="350"> |

> 转换后的 PPTX 已移除水印，并保留可编辑的文字图层。

### v1.0 - AI 智能去字

| 转换前 (NotebookLM PDF) | 转换后 (可编辑 PPTX) |
|:-----------------------:|:--------------------:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-before.png" width="400"> |

> 左：NotebookLM 原始 PDF（文字嵌入图片中）
> 右：转换后的 PPTX，干净背景 + 可编辑文字图层

## v2.0 新功能

### 重要提示 (v2.0.1)

由于图形识别 AI 决定了输出质量，**不建议一次转换多页**。建议**每次处理 5 张以内**，以获得最佳转换效果。

### 并行处理

文字移除与 OCR 识别现在同时执行，大幅缩短图片来源的处理时间。

### 批量处理与速率保护

每 3 页为一批次，批次间有 6 秒冷却时间，降低处理大量页面时的 API 429 错误。

### PDF OCR 回退机制

自动检测无文字层的 PDF（如 NotebookLM 导出文件），自动回退使用 OCR，确保所有 PDF 类型都能生成文字图层。

### 稳定性提升

- 改进文字移除，更好地保留非文字元素
- 新增验证机制，跳过无效的 OCR 结果
- 强化边缘案例的错误处理

## 功能特色

- **AI 智能去字**：使用 Gemini 2.5 Flash 自动移除图片中的文字并重建背景
- **混合文字提取**：PDF 来源使用 PDF.js 原生提取精确坐标；图片来源使用增强版 Gemini OCR
- **底图文字分离**：导出的 PPTX 中，背景图与文字为独立图层，方便后续编辑
- **批量处理**：支持多页 PDF 或多张图片一次处理
- **页面选择**：可自由勾选需要处理的页面，节省时间与 API 额度

## 使用方式

### 在 Google Gemini Canvas 中使用

1. 打开 [Google Gemini](https://gemini.google.com/)
2. 在对话中输入类似以下的提示：
   ```
   直接执行，不要修改
   ```
3. 当 Gemini 进入 **Canvas 模式**（屏幕右侧出现代码编辑区）
4. 将本项目的 `index-zh-CN.html`（或你偏好的语言版本）完整代码粘贴到 Canvas
5. 点击 Canvas 右上角的「**Preview**」按钮预览运行

### API Key 设置

> **重要提示**：在 Gemini Canvas 环境中运行时，**无需设置私人 API Key**。系统会自动使用默认的 API 环境。

如果你想在 Canvas 以外的环境运行（例如自建服务器），请在代码中找到以下位置，填入你的 Gemini API Key：

```javascript
const apiKey = "你的_GEMINI_API_KEY";
```

> 获取 API Key：前往 [Google AI Studio](https://aistudio.google.com/app/apikey) 创建

## 操作流程

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│  上传 PDF   │ -> │  选择页面   │ -> │  AI 处理    │ -> │  导出 PPTX  │
│  或图片     │    │  (可多选)   │    │  去字重建   │    │  底图+文字  │
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

### Step 1：上传文件
- 拖放或点击上传 NotebookLM 导出的 PDF
- 也支持 JPG、PNG、WebP 等图片格式
- 可一次上传多个文件

> **提示**：NotebookLM 导出的 PDF 文件尺寸可能较大，可以先使用免费 PDF 压缩服务缩小文件后再上传，处理效率会更好。

### Step 2：选择页面
- 系统自动生成所有页面的缩略图
- 勾选需要处理的页面（默认全选）
- 点击「开始处理」进入下一步

### Step 3：AI 处理
- Gemini 会逐页移除文字并重建背景
- 处理进度实时显示
- 每页约需 3-5 秒（含 API 延迟）

> **注意**：Gemini 去文字可能会有不完整的情况，如发现文字残留过多，可以重新处理一次。

### Step 4：导出 PPTX
- 选择演示文稿比例（16:9 / 9:16 / 4:3）
- 点击「合并 PPTX」导出
- 文字定位采用混合策略：
  - **PDF 来源**：使用 PDF.js 预先提取的坐标（即时完成，无需 API）
  - **图片来源**：使用 Gemini OCR 增强版样式检测

## 输出结果

导出的 PPTX 每页包含：

| 图层 | 内容 |
|------|------|
| 底层 | 去除文字后的干净背景图 |
| 上层 | 可编辑的文字框（位置对应原始文字） |

这样的分层结构让你可以：
- 轻松修改文字内容
- 更换字体、颜色、大小
- 调整文字位置
- 保留原始设计风格

## 技术规格

| 项目 | 说明 |
|------|------|
| AI 模型 | Gemini 2.5 Flash (Image Edit + Text Gen) |
| 文字移除 | 优化提示词实现完整去字与背景修复 |
| PDF 解析 | PDF.js 3.11.174 |
| PPTX 生成 | PptxGenJS 3.12.0 |
| 渲染分辨率 | 缩略图 0.5x / 处理 2.0x |
| 支持格式 | PDF, JPG, PNG, WebP, BMP |
| 文字提取 | 混合策略：PDF.js 原生（PDF）/ Gemini OCR（图片）|

## 注意事项

1. **API 额度**：文字移除使用 Gemini API；PDF 文字提取为本地处理（无 API 成本）
2. **Rate Limit**：若遇到 429 错误，系统会自动等待重试
3. **处理时间**：大量页面建议分批处理
4. **网络需求**：需要稳定的网络连接
5. **浏览器**：建议使用 Chrome 或 Edge 最新版本
6. **PDF 优势**：PDF 来源导出更快，文字定位更精准

## 常见问题

### Q: 为什么要用 Gemini Canvas？
A: Canvas 模式提供了一个安全的沙盒环境来运行前端代码，无需搭建服务器即可运行完整功能。而且会使用默认的 API 环境，无需设置私人 API Key。

### Q: 处理失败怎么办？
A: 常见原因：
- API Key 无效或过期（在 Canvas 以外运行时）
- 网络连接不稳定
- 图片过大或格式不支持
- API 调用频率过高（等待后重试）

### Q: 可以离线使用吗？
A: 不行，本工具需要调用 Gemini API 进行 AI 处理。

## 多语言版本

| 语言 | 文件 |
|------|------|
| 繁體中文 | `index.html` |
| English | `index-en.html` |
| Español | `index-es.html` |
| 日本語 | `index-ja.html` |
| Français | `index-fr.html` |
| 简体中文 | `index-zh-CN.html` |

## License

MIT License
