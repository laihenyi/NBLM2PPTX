# NBLM2PPTX - NotebookLM PDF 转 PPTX 工具

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

将 NotebookLM 导出的 PDF 转换成**底图与文字分离**的 PPTX 演示文稿。

> ✨ **更新 (2026-01-20)**: v2.2 版本发布 - 软重置保留 API Key！加上速度优化与 IMAGE_RECITATION 错误修复。

[English](README.md) | [繁體中文](README-zh-TW.md) | [日本語](README-ja.md) | [Español](README-es.md) | [Français](README-fr.md)

## 演示

### v1.1 - 混合文字提取

| 原始 (NotebookLM PDF) | 输出 (可编辑 PPTX) |
|:---------------------:|:------------------:|
| <img src="assets/demo-v1.1-original.jpg" width="400"> | <img src="assets/demo-v1.1-output.jpg" width="400"> |

> PDF.js 原生文字提取提供精确的文字定位，无需额外 API 调用。

### v1.0 - AI 智能去字

| 转换前 (NotebookLM PDF) | 转换后 (可编辑 PPTX) |
|:-----------------------:|:--------------------:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-before.png" width="400"> |

> 左：NotebookLM 原始 PDF（文字嵌入图片中）
> 右：转换后的 PPTX，干净背景 + 可编辑文字图层

## v2.2 版本更新 (2026-01-20)

### 🎯 软重置保留 API Key
- **无需重复输入**：点击「重新开始」时，API Key 会保留在内存中
- **无限次重新开始**：可处理多批文件而无需重新输入凭证
- **智能状态管理**：重置所有处理状态但保留您的 API Key

### ⚡ 速度优化
- **处理速度提升 70%**：页面间延迟从 3.5 秒降至 1.0 秒
- **并行处理**：充分利用并发 API 调用达到最高效率
- **即时重置**：软重置立即回到初始状态，无需重新加载页面

### 🔧 IMAGE_RECITATION 错误修复
- **改进 AI 提示词**：优化提示工程以避免版权检测
- **更佳背景重建**：更精确的内容感知填充结果
- **降低温度参数**：使用温度 0.4 获得更一致的 AI 行为

### 📝 UI 改进
- **更清晰的说明**：更新 API Key 设置指南以符合实际流程
- **干净的重置界面**：重置时恢复初始上传界面，而非加载动画

## 功能特色

- **AI 智能去字**：使用 Gemini 2.5 Flash 自动移除图片中的文字并重建背景
- **混合文字提取**：PDF 来源使用 PDF.js 原生提取精确坐标；图片来源使用增强版 Gemini OCR
- **底图文字分离**：导出的 PPTX 中，背景图与文字为独立图层，方便后续编辑
- **批量处理**：支持多页 PDF 或多张图片一次处理
- **页面选择**：可自由勾选需要处理的页面，节省时间与 API 额度

## 使用方式

### 快速开始（3 个简单步骤）

1. **在浏览器中打开 HTML 文件**（推荐 Chrome/Edge）
2. **按照引导设置**从 Google 获取免费 API Key
3. **开始处理** 您的 PDF 或图片！

### 首次设置

当您第一次打开应用程序时，友好的设置向导会引导您完成：

1. **访问 Google AI Studio** - 一键链接到 [aistudio.google.com/apikey](https://aistudio.google.com/apikey)
2. **创建免费 API Key** - 使用 Google 账户登录（无需信用卡）
3. **粘贴并保存** - 复制您的 API Key 并粘贴到应用中

> 🔒 **您的 API Key 安全存储在浏览器中**，永远不会上传到任何服务器。

### 免费 API 配额

Google Gemini API 提供慷慨的免费层级：
- **每分钟 15 个请求**
- **每天 1,500 个请求**
- **无需信用卡**

这对于日常使用来说绰绰有余！

### 备选方案：在 Google Gemini Canvas 中使用（高级）

如果您更喜欢在 Gemini Canvas 环境中运行：

1. 打开 [Google Gemini](https://gemini.google.com/)
2. 将 `index-zh-CN.html` 中的代码粘贴到 Canvas
3. 点击「Preview」运行

> ⚠️ **注意**：截至 2026 年 1 月，即使在 Canvas 环境中也需要 API Key。应用会提示您进行设置。

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
- 点击「导出 PPTX」下载
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
| AI 模型 | Gemini 2.5 Flash Image（文字移除）+ Gemini 2.5 Flash（OCR）|
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

### Q: 需要信用卡才能获取 API Key 吗？
A: 不需要！Google Gemini API 提供完全免费的层级，无需信用卡。只需使用 Google 账户登录即可。

### Q: 我的 API Key 安全吗？
A: 是的！您的 API Key 仅存储在浏览器的 localStorage 中，除了 Google 官方 Gemini API 外，不会发送到任何服务器。

### Q: 处理失败怎么办？
A: 常见原因：
- API Key 无效（检查是否以 "AIza" 开头）
- 网络连接不稳定
- 图片过大或格式不支持
- API 频率限制超出（免费层级：15/分钟，1500/天 - 等待后重试）

### Q: 可以与他人分享这个工具吗？
A: 当然可以！只需分享 HTML 文件。每个用户将设置自己的 API Key，因此每个人都有自己的免费配额。

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
