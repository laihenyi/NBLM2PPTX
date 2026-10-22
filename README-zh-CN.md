# NBLM2PPTX - NotebookLM PDF 转 PPTX 工具

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

将 NotebookLM 导出的 PDF 转换成**底图与文字分离**的 PPTX 演示文稿。

> ✨ **更新 (2026-10-21)**: macOS 原生版 NBLM2PPTX v0.1.1 发布——在 Mac 本机把 PDF 转换为可编辑 PPTX，无需 API key，已通过 Apple 公证。详见下方“macOS 原生版”。

[English](README.md) | [繁體中文](README-zh-TW.md) | [日本語](README-ja.md) | [Español](README-es.md) | [Français](README-fr.md)

## macOS 原生版（离线，Rust）

**NBLM2PPTX for macOS** 是在 Mac 本机运行的原生 App：不需要 API key，文件不上传。源代码在 [`macos-app/`](macos-app/)。

- **下载**：[Releases 页面](https://github.com/laihenyi/NBLM2PPTX/releases/tag/n2p-rs-v0.1.1)（`NotebookLM2PPT-notarized.zip`）
- **OCR**：macOS 内置 Vision（设备端）
- **去字**：只擦除文字笔画；有纹理的背景由 LaMa 在 GPU（CoreML）上重建
- **对象提取（实验性）**：图标、插图变成可移动的图片（MobileSAM）
- **App**：拖放式图形界面，Developer ID 签名并通过 Apple 公证；模型（约 253 MB）在首次打开时下载
- **需求**：Apple Silicon；GPU 推理需 macOS 12 以上

构建、命令行用法与设计笔记见 [`macos-app/README.md`](macos-app/README.md)；第三方许可见 [`macos-app/THIRD-PARTY.md`](macos-app/THIRD-PARTY.md)。

### 前后对比（标准版）

| 原稿（NotebookLM PDF） | 输出（可编辑 PPTX，macOS App） |
|:---:|:---:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-native-v0.1.1-alpha6.jpg" width="400"> |
| <img src="assets/demo-v1.1-original.jpg" width="400"> | <img src="assets/demo-native-v0.1.1-kitchen.jpg" width="400"> |

> 文字变成可编辑的文本框；插图保留在背景中，NotebookLM 水印已移除（输出用 LibreOffice 渲染）。
> 图中可见的现有限制：荧光标记色不保留、粗体偶尔丢失、OCR 偶有错字。

### v0.1.1（2026-10-21）

- **对象版快约一倍，内存峰值从约 6 GB 降到约 1.9 GB**：SAM 改为只在 CPU 上运行（11 页演示文稿 47–86 秒，原本 103–183 秒）
- 多行正文按原稿左对齐（原本一律居中）
- 对象提取的掩码膨胀加速
- 图形界面：模型区域显示 LaMa 的 GPU 版是否就绪
- 开发工具：PowerPoint 兼容性检查只读取、只关闭自己打开的演示文稿

### v0.1.0（2026-10-21）

- 首次发布：拖放式 App，已签名并通过公证
- 速度（Apple Silicon，11 页演示文稿）：标准版 29–45 秒（约 1.1–1.4 GB），对象版 103–183 秒（约 6 GB）

## Gemini 辅助网页版（已停止更新）

早期的 Gemini 辅助网页版（`index.html` 与各语言版本、演示视频项目）已归档在 [`legacy-web/`](legacy-web/)，**不再更新**；需要的开发者可自行取用。说明文档：[`legacy-web/README-zh-CN.md`](legacy-web/README-zh-CN.md)。

## License

MIT License
