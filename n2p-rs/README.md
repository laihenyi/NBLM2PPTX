# n2p-rs

NotebookLM2PPT 的 Rust 改寫，**目標平台是 macOS**。

Windows 版靠「微軟電腦管家智慧圈選 + 模擬滑鼠截圖」取得可編輯文字；macOS 上沒有這些東西，
因此這裡把那一整層換掉：

| Windows 版 | n2p-rs |
| :--- | :--- |
| PyMuPDF 點陣化 | `n2p-pdf`（hayro，純 Rust） |
| numpy／scipy 去水印 | `n2p-image`（4 種逐位元等價 + 2 種替代） |
| 微軟電腦管家智慧圈選 OCR | `n2p-vision`（macOS 內建 Vision，裝置端、免上傳） |
| Spire.Presentation 產生 PPTX | `n2p-ooxml`（自建 OPC + OOXML 寫入器） |
| 逐頁截圖後合併單頁 PPTX | `n2p-build` 直接產生整份簡報，不需要合併 |
| tkinter GUI | `n2p-cli`（clap） |
| PyInstaller 打包 | `cargo build --release` 單一執行檔 |

## 快速開始

```bash
cd n2p-rs
cargo test                                    # 123 個測試
cargo build --release -p n2p-cli
./target/release/n2p --help
```

### 轉換一份 PDF

```bash
# 最基本：Vision OCR + 去水印 + 16:9
n2p convert deck.pdf -o deck.pptx

# 只用圖片（不做 OCR，每頁一張滿版圖）
n2p convert deck.pdf --layout image

# 用 MinerU 的 JSON 當版面來源（語意標籤較完整，可分辨標題／內文）
n2p convert deck.pdf --layout mineru --mineru deck.json

# 只轉前 3 頁、用洋蔥皮修補、保留中間產物
n2p convert deck.pdf --pages 1-3 --method onion --work-dir ./work

# 讓 PowerPoint 自動縮字，並保留高解析度背景
n2p convert deck.pdf --autofit --high-res-background
```

### 診斷

```bash
n2p info deck.pdf          # 頁數、頁面尺寸、各 DPI 的輸出像素
n2p methods                # 列出六種去水印方法
n2p fonts                  # 候選東亞字型與安裝狀態（決定 <a:ea> 用哪個）
n2p ocr page.png           # 單張影像的 Vision 辨識結果與邊界框
```

### 驗證工具（xtask）

```bash
cargo run -p xtask -- doctor        # 檢查 PowerPoint / LibreOffice 是否可用

# PowerPoint 讀得懂我們產生的 OOXML 嗎？（即時產生簡報再開檔驗證）
cargo run -p xtask -- compat

# 對既有檔案做基準回歸
cargo run -p xtask -- compat --record --dir ../rust-rewrite/assets/probe-pptx
cargo run -p xtask -- compat --dir ../rust-rewrite/assets/probe-pptx

# 像素級視覺回歸（需要 LibreOffice）
cargo run -p xtask -- visual-diff --dir ../rust-rewrite/assets/probe-pptx --record
cargo run -p xtask -- visual-diff --dir ../rust-rewrite/assets/probe-pptx
```

`visual-diff` 的基準圖在 `rust-rewrite/assets/visual/`，旁邊的 `meta.json` 記著產生它的
LibreOffice 版本與 DPI；環境不符時會先警告再比對。實測 LibreOffice 的渲染逐位元可重現
（25 頁平均差全部 0.00），所以任何非零差異都是真的改到了東西。

`compat` 只會關閉自己開啟的文件，永不碰使用者在 PowerPoint 裡的其他簡報，
也不會呼叫 `quit`。macOS 上 PowerPoint 的 AppleScript 有若干陷阱
（拿不到畫布高度、無匯出 PDF 命令、集合迭代會卡死、相對路徑會彈 modal），
全部記錄在 [`../rust-rewrite/04-powerpoint-automation.md`](../rust-rewrite/04-powerpoint-automation.md)。

## 模組

| crate | 職責 | 與 Python 版的關係 |
| :--- | :--- | :--- |
| `n2p-core` | `RgbImage`、`BBox`（含 IOU） | 共用基礎 |
| `n2p-pdf` | PDF → PNG，頁數／頁面尺寸 | 取代 PyMuPDF |
| `n2p-image` | 邊緣色彩統計、去水印、寬屏化 | 取代 `edge_diversity.py`、`inpaint_methods.py`、`image_inpainter.py` |
| `n2p-layout` | 中立的版面模型 | 新增，用來隔離「版面來源」 |
| `n2p-mineru` | MinerU／Paddle JSON → 版面模型 | 取代 `ppt_refiner.py` 的解析段 |
| `n2p-vision` | macOS Vision OCR → 版面模型 | **取代電腦管家智慧圈選** |
| `n2p-ooxml` | OPC + `p:sp`/`p:pic`/`p:bg` + 合併 | 取代 Spire.Presentation |
| `n2p-build` | 版面 + 頁面影像 → PPTX | 取代 `ppt_creater.py` |
| `n2p-pipeline` | 串接整條流程 | 取代 `cli.py` |
| `n2p-cli` | `n2p` 命令 | 取代 GUI |

## 驗證方式

不是「跑起來看起來對」，而是與參考實作逐項比對：

| 測試 | 參考來源 | 結果 |
| :--- | :--- | :--- |
| `n2p-ooxml/tests/golden.rs` | Spire.Presentation 11.8.1 的實際輸出 | `<p:sp>`／`<p:pic>`／`<p:bg>` 逐節點逐屬性相同 |
| `n2p-ooxml/tests/merge.rs` | 同上（`4_merged.pptx`） | 3 份合併 → 1 master / 11 layouts / 1 theme，30,677 bytes（Spire 是 74,295） |
| `n2p-pdf/tests/reference.rs` | PyMuPDF | 頁數、頁面尺寸、像素尺寸（2867×1600）完全相同 |
| `n2p-image/tests/golden.rs` | numpy 實作 | 4 種方法**逐像素相同**（0 個不同像素） |
| `n2p-pipeline` | 端到端 | Vision 實際辨識出頁面中文標題，位置與 `ppt_creater` 語意一致 |
| `xtask compat` | Microsoft PowerPoint 16.101 | 我們產生的 pptx 開得起來，頁數／寬度／形狀名稱與文字都正確 |
| `xtask visual-diff` | LibreOffice 26.8.1.1 | 17 份 fixture、25 頁，平均差全部 0.00 |

交叉驗證：產出的 PPTX 另以 `python-pptx`（與 Spire、與本專案都無關的第三個實作）讀取，
確認畫布、背景填滿、文字位置與內容都正確。

### 幾何關鍵

`image_inpainter.py` 的水印座標硬編碼以 `2867 × 1600` 為基準（1376×768 pt @ 150 dpi）。
`n2p-pdf` 因此把像素尺寸改成**向上取整**（與 MuPDF 一致）；用截斷會得到 2866，差一個像素。
這一條有專門的測試鎖住。

## 與 Python 版的行為差異

| 項目 | Python 版 | n2p-rs | 理由 |
| :--- | :--- | :--- | :--- |
| 評估水印 | Spire 未授權時每頁附加警告形狀，靠 `clean_ppt` 事後刪除 | 沒有水印可刪 | 自己寫 XML |
| 合併 | 每份來源各帶一套 master/layout/theme | 共用基底 | 來源同構，去重後小 60% |
| 多行文字 | 換行字元塞進 `<a:t>` | `<a:br/>`（可用 `Literal` 還原） | 規格正確寫法，視覺相同 |
| 文字溢出 | `FitTextToShape` 實際不寫任何 autofit | 預設相同，`--autofit` 可修 | 等價優先，改善選用 |
| 背景擦除 | 把文字區塊整塊填成邊緣主色 | 預設**只修補文字筆畫**，並依背景平坦度自動選引擎：平坦處插值填色、有紋理處交給 LaMa 重建（`--erase auto`）；另有 `stroke`／`lama`／`inpaint`／`fill` | 用矩形當遮罩必然連子彈點、光暈、圖示一起抹掉 |
| 字級估算 | `bbox高度 × 0.75` | 中文用字寬（`em ≈ 行寬 / 全形字數`），多行用行距 | 舊公式對 OCR 的墨跡框系統性偏小 30% |
| 文字框尺寸 | 直接用 bbox | 依「字級 × 行數」重算，並關閉自動折行 | bbox 貼齊墨跡，裝不下 PowerPoint 的行距 |
| 東亞字型 | 只寫 `<a:latin>` | 中文簡報自動偵測並補 `<a:ea>`（本機為 **Noto Sans TC**） | CJK 字元走 `a:ea`，不指定就落到主題字型 |
| `docProps/app.xml` | 16:9 寫成「全屏显示(4:3)」、3 頁寫 `Slides=1` | 寫對 | Spire 的 bug，不重現 |
| `griddata`／`skimage` 修補 | SciPy／scikit-image | IDW／調和插值 | Rust 無等價物；視覺接近，用 `n2p methods` 可查 |

## 文字遮罩與去字修補

遮罩不是「矩形」，也不是「顏色距離」，而是 **Otsu ∪ 局部自適應** 的逐像素筆畫遮罩
（做法參考 `Appt-OCR` 的 `analyze_text_features`）：

1. **極性由框線亮度決定**（`border_mean < 127` → 淺色字）。比「取樣文字色再比亮度」穩，
   因為取樣本身就會被光暈帶偏。
2. **全域 Otsu ∪ 局部自適應**（積分影像取局部均值）。兩者互補：Otsu 抓對比強的字，
   自適應抓漸層、光束、木紋上的字。
3. **形態學閉運算**補斷筆。核大小用**單行字高**（`em/25`，上限 4 px）——
   用整個區塊高度會讓多行標題的核大到把筆畫糊成一團。
4. **往對比相鄰像素擴張**幾輪，吃掉抗鋸齒亮邊、深色投影與白色描邊（雙向）。

修補引擎：

| 引擎 | 適用 | 行為 |
| :--- | :--- | :--- |
| 插值（`BackgroundSmooth`） | 平坦背景（色塊、漸層） | 從外環取主色再向外擴散；遮罩擴到 `0.30 × 字高`，把柔邊光暈一起補掉 |
| **LaMa**（`--erase lama`／有模型時的 `auto`） | 木紋、照片、複雜紋理 | ONNX Runtime 本機推論，512×512 圖塊 + 羽化加權混合；遮罩只擴 3–6 px |

平坦／有紋理的判斷用文字外圍一圈的**亮度中位數絕對偏差**（MAD < 6 視為平坦）。
用 MAD 而不是標準差，是因為環上常夾著裝飾（引號、標靶圖案），標準差會被那幾個
極端值撐大。

LaMa 模型（Apache-2.0，約 199 MiB）不進版控，第一次使用前抓一次：

```bash
n2p models fetch     # 下載到 ~/Library/Caches/n2p/lama_fp32.onnx
n2p models status    # 檢查是否就緒
```

沒有模型時 `--erase auto` 會自動退回只擦筆畫的插值模式，不會失敗。

## 已知限制

1. **Vision 不給語意標籤。** 所有文字都建成內文框，不會自動判定「這是標題」。需要 `paragraph_title`
   這類語意時請走 `--layout mineru`。
2. **Vision 不做圖片區塊偵測。** 圖片留在背景圖上不被擦除；視覺結果與 Windows 版一致
   （Windows 版也是「文字變文字框、其餘留背景」）。
3. **去水印只對 NotebookLM 的固定位置有效。** 這是刻意保留的耦合，改成自動偵測會讓位置跑掉。
4. **非 macOS 平台可編譯但沒有 OCR。** `--layout auto` 會退回 `image`。

## 目錄

```
crates/
├── n2p-core/       RgbImage、BBox、IOU
├── n2p-pdf/        hayro 封裝
├── n2p-image/      diversity / inpaint / watermark / transform
├── n2p-layout/     中立版面模型 + LayoutProvider trait
├── n2p-mineru/     MinerU / Paddle JSON 解析
├── n2p-vision/     macOS Vision OCR
├── n2p-ooxml/      OPC / XML DOM / p:sp / p:pic / p:bg / merge / props
├── n2p-build/      版面 + 影像 → PPTX（內嵌空白範本）
├── n2p-pipeline/   流程編排
└── n2p-cli/        n2p 命令
```

規格文件在 [`../rust-rewrite/`](../rust-rewrite/)。
