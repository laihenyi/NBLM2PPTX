# NBLM2PPTX - NotebookLM PDF to PPTX Converter

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

Convert NotebookLM exported PDFs into PPTX presentations with **separated background images and editable text layers**.

> ✨ **Updated (2026-10-21)**: NBLM2PPTX for macOS v0.1.1 — offline PDF → editable PPTX on your Mac, no API key, notarized by Apple. See “macOS Native App” below.

[繁體中文](README-zh-TW.md) | [简体中文](README-zh-CN.md) | [日本語](README-ja.md) | [Español](README-es.md) | [Français](README-fr.md)

## macOS Native App (offline, Rust)

**NBLM2PPTX for macOS** is a native app that runs entirely on your Mac — no API key, nothing uploaded. Source code: [`macos-app/`](macos-app/).

- **Download**: [release page](https://github.com/laihenyi/NBLM2PPTX/releases/tag/n2p-rs-v0.1.1) (`NotebookLM2PPT-notarized.zip`)
- **OCR**: macOS built-in Vision (on-device)
- **Text removal**: erases only the text strokes; textured backgrounds are rebuilt by LaMa running on the GPU (CoreML)
- **Object extraction (experimental)**: icons and illustrations become movable images (MobileSAM)
- **App**: drag-and-drop GUI, signed with Developer ID and notarized by Apple; models (~253 MB) are downloaded on first launch
- **Requirements**: Apple Silicon; macOS 12+ for GPU inference

Build, CLI usage and design notes: [`macos-app/README.md`](macos-app/README.md). Third-party licenses: [`macos-app/THIRD-PARTY.md`](macos-app/THIRD-PARTY.md).

### Before / After (standard version)

| Original (NotebookLM PDF) | Output (editable PPTX, macOS app) |
|:---:|:---:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-native-v0.1.1-alpha6.jpg" width="400"> |
| <img src="assets/demo-v1.1-original.jpg" width="400"> | <img src="assets/demo-native-v0.1.1-kitchen.jpg" width="400"> |

> Text becomes editable text boxes; illustrations stay in the background and the NotebookLM watermark is removed (output rendered with LibreOffice).
> Current limitations visible here: highlight colors are not kept, bold is sometimes lost, and OCR may misread a few characters.

### v0.1.1 (2026-10-21)

- **Object version about 2× faster, peak memory ~6 GB → ~1.9 GB**: SAM now runs on the CPU only (11-page deck: 47–86 s, was 103–183 s)
- Multi-line body text is left-aligned when the original is (it used to be always centered)
- Faster mask dilation for object extraction
- GUI: the model section shows whether the GPU version of LaMa is ready
- Developer tooling: the PowerPoint compatibility check only reads and closes the presentation it opened itself

### v0.1.0 (2026-10-21)

- First release: drag-and-drop app, signed and notarized
- Speed (Apple Silicon, 11-page deck): standard version 29–45 s (~1.1–1.4 GB), object version 103–183 s (~6 GB)

## Gemini-Assisted Web Version (no longer updated)

The earlier Gemini-assisted web version (`index.html` and its language versions, demo video project) is archived in [`legacy-web/`](legacy-web/). It is **no longer updated**; developers who need it can take it from there. Its documentation: [`legacy-web/README.md`](legacy-web/README.md).

## License

MIT License
