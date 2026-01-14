# NBLM2PPTX - NotebookLM PDF を PPTX に変換

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![GitHub stars](https://img.shields.io/github/stars/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/network/members)
[![GitHub issues](https://img.shields.io/github/issues/laihenyi/NBLM2PPTX)](https://github.com/laihenyi/NBLM2PPTX/issues)

NotebookLM からエクスポートした PDF を**背景画像とテキストが分離された** PPTX プレゼンテーションに変換します。

[English](README.md) | [繁體中文](README-zh-TW.md) | [简体中文](README-zh-CN.md) | [Español](README-es.md) | [Français](README-fr.md)

## デモ

### v1.1 - ハイブリッドテキスト抽出

| 原本 (NotebookLM PDF) | 出力 (編集可能な PPTX) |
|:---------------------:|:----------------------:|
| <img src="assets/demo-v1.1-original.jpg" width="350"> | <img src="assets/demo-v1.1-output.jpg" width="350"> |

> 変換後の PPTX は透かしが削除され、編集可能なテキストレイヤーを保持。

### v1.0 - AI テキスト削除

| 変換前 (NotebookLM PDF) | 変換後 (編集可能な PPTX) |
|:-----------------------:|:------------------------:|
| <img src="assets/demo-after.png" width="400"> | <img src="assets/demo-before.png" width="400"> |

> 左：NotebookLM の元の PDF（テキストが画像に埋め込まれている）
> 右：変換後の PPTX、クリーンな背景 + 編集可能なテキストレイヤー

## v2.0 の新機能

### 重要なお知らせ (v2.0.1)

画像認識 AI が出力品質を決定するため、**一度に多くのページを変換することはお勧めしません**。最適な結果を得るには、**1回あたり5ページ以内**で処理してください。

### 並列処理

テキスト削除と OCR 認識が同時に実行されるようになり、画像ソースの処理時間が大幅に短縮されました。

### レート制限保護付きバッチ処理

3ページごとにバッチ処理され、バッチ間に6秒のクーリング期間があり、大量のPDFを処理する際のAPI 429エラーを削減します。

### PDF の OCR フォールバック

抽出可能なテキストレイヤーのない PDF（NotebookLM エクスポートなど）を自動的に検出し、OCR にフォールバックして、すべての PDF タイプでテキストレイヤーの生成を保証します。

### 安定性の向上

- テキスト削除の改善、非テキスト要素のより良い保持
- 無効な OCR 結果をスキップする検証を追加
- エッジケースに対するより堅牢なエラー処理

## 機能

- **AI テキスト削除**：Gemini 2.5 Flash を使用して画像からテキストを自動的に削除し、背景を再構築
- **ハイブリッドテキスト抽出**：PDF ソースは PDF.js ネイティブ抽出で正確な座標を取得；画像ソースは強化版 Gemini OCR を使用
- **レイヤー分離**：エクスポートされた PPTX には、背景画像とテキストが独立したレイヤーとして含まれ、編集が容易
- **バッチ処理**：複数の PDF ページまたは画像を一度に処理可能
- **ページ選択**：処理するページを自由に選択でき、時間と API クォータを節約

## 使用方法

### Google Gemini Canvas での使用

1. [Google Gemini](https://gemini.google.com/) を開く
2. 以下のようなプロンプトを入力：
   ```
   そのまま実行してください。修正は不要です
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

> **ヒント**：NotebookLM からエクスポートした PDF はサイズが大きい場合があります。無料の PDF 圧縮サービスでファイルサイズを縮小してからアップロードすると、処理効率が向上します。

### Step 2：ページの選択
- システムが自動的にすべてのページのサムネイルを生成
- 処理するページにチェックを入れる（デフォルトですべて選択）
- 「処理開始」をクリックして次へ

### Step 3：AI 処理
- Gemini が各ページからテキストを削除し、背景を再構築
- 進捗状況がリアルタイムで表示
- 各ページ約 3-5 秒（API レイテンシを含む）

> **注意**：Gemini のテキスト削除は不完全な場合があります。テキストの残留が多い場合は、再処理をお試しください。

### Step 4：PPTX のエクスポート
- プレゼンテーションの比率を選択（16:9 / 9:16 / 4:3）
- 「PPTX エクスポート」をクリックしてダウンロード
- テキスト配置はハイブリッド戦略を使用：
  - **PDF ソース**：PDF.js で事前抽出した座標を使用（即時完了、API 不要）
  - **画像ソース**：Gemini OCR 強化版スタイル検出を使用

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
| テキスト削除 | 完全な削除と背景修復のための最適化プロンプト |
| PDF 解析 | PDF.js 3.11.174 |
| PPTX 生成 | PptxGenJS 3.12.0 |
| レンダリング解像度 | サムネイル 0.5x / 処理 2.0x |
| サポート形式 | PDF, JPG, PNG, WebP, BMP |
| テキスト抽出 | ハイブリッド：PDF.js ネイティブ（PDF）/ Gemini OCR（画像）|

## 注意事項

1. **API クォータ**：テキスト削除は Gemini API を使用；PDF テキスト抽出はローカル処理（API コストなし）
2. **レート制限**：429 エラーが発生した場合、システムは自動的に待機して再試行
3. **処理時間**：大量のページがある場合はバッチ処理を推奨
4. **ネットワーク**：安定したインターネット接続が必要
5. **ブラウザ**：Chrome または Edge（最新バージョン）を推奨
6. **PDF の利点**：PDF ソースはエクスポートが高速で、テキスト配置がより正確

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
