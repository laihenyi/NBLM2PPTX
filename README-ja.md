# NBLM2PPTX - NotebookLM PDF を PPTX に変換

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

NotebookLM からエクスポートした PDF を**背景画像とテキストが分離された** PPTX プレゼンテーションに変換します。

> ✨ **更新 (2026-10-21)**: macOS ネイティブ版 NBLM2PPTX v0.1.1 をリリース。Mac 上だけで PDF を編集可能な PPTX に変換。API キー不要、Apple の公証済み。詳しくは下の「macOS ネイティブ版」を参照してください。

[English](README.md) | [繁體中文](README-zh-TW.md) | [简体中文](README-zh-CN.md) | [Español](README-es.md) | [Français](README-fr.md)

## macOS ネイティブ版（オフライン、Rust）

**NBLM2PPTX for macOS** は Mac 上だけで動くネイティブアプリです。API キーは不要で、ファイルはアップロードされません。ソースコードは [`macos-app/`](macos-app/) にあります。

- **ダウンロード**：[Releases ページ](https://github.com/laihenyi/NBLM2PPTX/releases/tag/n2p-rs-v0.1.1)（`NotebookLM2PPT-notarized.zip`）
- **OCR**：macOS 内蔵の Vision（デバイス上で処理）
- **文字消去**：文字のストロークだけを消し、テクスチャのある背景は GPU（CoreML）上の LaMa で復元
- **オブジェクト切り出し（実験的）**：アイコンやイラストを移動可能な画像に（MobileSAM）
- **アプリ**：ドラッグ＆ドロップの GUI。Developer ID で署名し、Apple の公証済み。モデル（約 253 MB）は初回起動時にダウンロード
- **動作要件**：Apple Silicon。GPU 推論には macOS 12 以降

ビルド方法、コマンドラインの使い方、設計メモは [`macos-app/README.md`](macos-app/README.md)、サードパーティのライセンスは [`macos-app/THIRD-PARTY.md`](macos-app/THIRD-PARTY.md) を参照してください。

### 変換前後の比較（標準版）

| 元の PDF（NotebookLM） | 出力（編集可能な PPTX、macOS アプリ） |
|:---:|:---:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-native-v0.1.1-alpha6.jpg" width="400"> |
| <img src="assets/demo-v1.1-original.jpg" width="400"> | <img src="assets/demo-native-v0.1.1-kitchen.jpg" width="400"> |

> 文字は編集可能なテキストボックスになり、イラストは背景に残り、NotebookLM の透かしは除去されます（出力は LibreOffice で描画）。
> 図で確認できる現在の制限：蛍光マーカーの色は保持されず、太字が失われることがあり、OCR がまれに誤認識します。

### v0.1.1（2026-10-21）

- **オブジェクト版が約 2 倍速く、ピークメモリは約 6 GB → 約 1.9 GB**：SAM を CPU のみで実行（11 ページのスライドで 47–86 秒、以前は 103–183 秒）
- 複数行の本文は元のスライドどおり左揃えに（以前は常に中央揃え）
- オブジェクト切り出しのマスク膨張を高速化
- GUI：モデル欄に LaMa の GPU 版が準備済みかを表示
- 開発ツール：PowerPoint 互換性チェックは自分で開いたプレゼンテーションだけを読み取り、閉じる

### v0.1.0（2026-10-21）

- 初回リリース：ドラッグ＆ドロップのアプリ、署名・公証済み
- 速度（Apple Silicon、11 ページのスライド）：標準版 29–45 秒（約 1.1–1.4 GB）、オブジェクト版 103–183 秒（約 6 GB）

## Gemini 支援 Web 版（更新終了）

以前の Gemini 支援 Web 版（`index.html` と各言語版、デモ動画プロジェクト）は [`legacy-web/`](legacy-web/) にアーカイブしました。**今後は更新されません**。必要な開発者はそこから利用してください。説明：[`legacy-web/README-ja.md`](legacy-web/README-ja.md)。

## License

MIT License
