# NBLM2PPTX - NotebookLM PDF を PPTX に変換

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

NotebookLM からエクスポートした PDF を**背景画像とテキストが分離された** PPTX プレゼンテーションに変換します。

[English](README.md) | [繁體中文](README-zh-TW.md) | [简体中文](README-zh-CN.md) | [Español](README-es.md) | [Français](README-fr.md)

## デモ

| 変換前 (NotebookLM PDF) | 変換後 (編集可能な PPTX) |
|:-----------------------:|:------------------------:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-before.png" width="400"> |

> 左：NotebookLM の元の PDF（テキストが画像に埋め込まれている）
> 右：変換後の PPTX、クリーンな背景 + 編集可能なテキストレイヤー

## 機能

- **AI テキスト削除**：Gemini 2.5 Flash を使用して画像からテキストを自動的に削除し、背景を再構築
- **OCR テキスト位置特定**：元のテキストの位置、サイズ、色を正確に認識
- **レイヤー分離**：エクスポートされた PPTX には、背景画像とテキストが独立したレイヤーとして含まれ、編集が容易
- **バッチ処理**：複数の PDF ページまたは画像を一度に処理可能
- **ページ選択**：処理するページを自由に選択でき、時間と API クォータを節約

## 使用方法

### Google Gemini Canvas での使用

1. [Google Gemini](https://gemini.google.com/) を開く
2. 以下のようなプロンプトを入力：
   ```
   PDF を PPTX に変換する Web ツールを作成してください
   ```
3. Gemini が **Canvas モード**に入ったら（右側にコードエディタが表示される）
4. プロジェクトの `index-ja.html`（または好みの言語バージョン）の完全なコードを Canvas に貼り付け
5. Canvas の右上にある「**Preview**」ボタンをクリックして実行

### API Key の設定

> **重要**：Gemini Canvas 環境で実行する場合、**個人の API Key は不要**です。システムはデフォルトの API 環境を自動的に使用します。

Canvas 以外の環境（例：自前のサーバー）で実行する場合は、コード内の以下の行を見つけて Gemini API Key を入力してください：

```javascript
const apiKey = "YOUR_GEMINI_API_KEY";
```

> API Key の取得：[Google AI Studio](https://aistudio.google.com/app/apikey) にアクセス

## ワークフロー

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│ PDF/画像を  │ -> │  ページを   │ -> │  AI 処理    │ -> │ PPTX を     │
│ アップロード│    │   選択      │    │ テキスト削除│    │ エクスポート│
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

### Step 1：ファイルのアップロード
- NotebookLM からエクスポートした PDF をドラッグ＆ドロップまたはクリックしてアップロード
- JPG、PNG、WebP などの画像形式もサポート
- 複数のファイルを一度にアップロード可能

### Step 2：ページの選択
- システムが自動的にすべてのページのサムネイルを生成
- 処理するページにチェックを入れる（デフォルトですべて選択）
- 「処理開始」をクリックして次へ

### Step 3：AI 処理
- Gemini が各ページからテキストを削除し、背景を再構築
- 進捗状況がリアルタイムで表示
- 各ページ約 3-5 秒（API レイテンシを含む）

### Step 4：PPTX のエクスポート
- プレゼンテーションの比率を選択（16:9 / 9:16 / 4:3）
- 「PPTX エクスポート」をクリックしてダウンロード
- エクスポート時に OCR を再度実行してテキストを正確に配置

## 出力構造

エクスポートされた PPTX の各スライドには以下が含まれます：

| レイヤー | 内容 |
|----------|------|
| 下層 | テキストが削除されたクリーンな背景画像 |
| 上層 | 編集可能なテキストボックス（元のテキスト位置に配置） |

このレイヤー構造により：
- テキスト内容を簡単に編集
- フォント、色、サイズの変更
- テキスト位置の調整
- 元のデザインスタイルの保持

## 技術仕様

| 項目 | 説明 |
|------|------|
| AI モデル | Gemini 2.5 Flash (Image Edit + Text Gen) |
| PDF 解析 | PDF.js 3.11.174 |
| PPTX 生成 | PptxGenJS 3.12.0 |
| レンダリング解像度 | サムネイル 0.5x / 処理 2.0x |
| サポート形式 | PDF, JPG, PNG, WebP, BMP |

## 注意事項

1. **API クォータ**：Canvas 以外で実行する場合、各処理で Gemini API を呼び出すため使用量に注意
2. **レート制限**：429 エラーが発生した場合、システムは自動的に待機して再試行
3. **処理時間**：大量のページがある場合はバッチ処理を推奨
4. **ネットワーク**：安定したインターネット接続が必要
5. **ブラウザ**：Chrome または Edge（最新バージョン）を推奨

## よくある質問

### Q: なぜ Gemini Canvas を使用するのですか？
A: Canvas モードは、サーバーを設定せずにフロントエンドコードを実行できる安全なサンドボックス環境を提供します。さらに、デフォルトの API 環境を使用するため、個人の API Key は不要です。

### Q: 処理が失敗した場合は？
A: 一般的な原因：
- API Key が無効または期限切れ（Canvas 以外で実行時）
- ネットワーク接続が不安定
- 画像が大きすぎるまたはサポートされていない形式
- API レート制限を超過（待機後に再試行）

### Q: オフラインで使用できますか？
A: いいえ、このツールは AI 処理のために Gemini API 呼び出しが必要です。

## 言語バージョン

| 言語 | ファイル |
|------|----------|
| 繁體中文 | `index.html` |
| English | `index-en.html` |
| Español | `index-es.html` |
| 日本語 | `index-ja.html` |
| Français | `index-fr.html` |
| 简体中文 | `index-zh-CN.html` |

## License

MIT License
