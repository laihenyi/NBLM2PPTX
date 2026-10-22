# n2p-rs

NotebookLM2PPT 的 Rust 改寫，**目標平台是 macOS**。

> 來源：本目錄原本開發於上游 [elliottzheng/NotebookLM2PPT](https://github.com/elliottzheng/NotebookLM2PPT)
> （MIT）的 clone 中，2026-10-21 連同 commit 歷史移入 NBLM2PPTX。移植自上游的模組、
> 測試資料與模型授權見 [`THIRD-PARTY.md`](THIRD-PARTY.md)。設計筆記在 [`docs/`](docs/)，
> 測試資料在 [`fixtures/`](fixtures/)。

Windows 版靠「微軟電腦管家智慧圈選 + 模擬滑鼠截圖」取得可編輯文字；macOS 上沒有這些東西，
因此這裡把那一整層換掉：

| Windows 版 | n2p-rs |
| :--- | :--- |
| PyMuPDF 點陣化 | `n2p-pdf`（hayro，純 Rust） |
| numpy／scipy 去水印 | `n2p-image`（4 種逐位元等價 + 2 種替代） |
| 微軟電腦管家智慧圈選 OCR | `n2p-vision`（macOS 內建 Vision，裝置端、免上傳） |
| Spire.Presentation 產生 PPTX | `n2p-ooxml`（自建 OPC + OOXML 寫入器） |
| 逐頁截圖後合併單頁 PPTX | `n2p-build` 直接產生整份簡報，不需要合併 |
| tkinter GUI | `n2p-gui`（egui）＋ `n2p-cli`（clap） |
| PyInstaller 打包 | `cargo build --release` 單一執行檔 |

## 快速開始

```bash
cd n2p-rs
cargo test                                    # 123 個測試
cargo build --release -p n2p-cli
./target/release/n2p --help
```

### 圖形介面

```bash
cargo run --release -p n2p-gui
```

1. 把 PDF 拖進視窗（可一次多份），或按「加入 PDF…」。
2. 選輸出版本：
   - **標準版（建議）**：文字變成文字框，圖片留在背景。輸出 `<檔名>_可編輯.pptx`。
   - **物件版（實驗性）**：另把圖示、插圖切成可移動的圖片（見下方「物件切出」）。
     輸出 `<檔名>_可編輯_物件.pptx`。
   - **兩種都產生**。
3. 選輸出位置（預設與 PDF 同資料夾），按「開始轉換」。

與 `n2p convert` 走同一條 pipeline，輸出逐位元組相同，速度與記憶體需求也相同。
GUI 的預設等於 `--high-res-background`（可在「進階設定」關閉）。

- 一次只轉一份；多份會排隊。11 頁的簡報實測（GPU）：標準版 29–45 秒、峰值約 1.1–1.4 GB；
  物件版 47–86 秒、峰值約 1.9 GB。
- 已存在的輸出檔預設跳過；勾選「覆寫已存在的檔案」才會重做。
  PowerPoint 正開著的檔案（同目錄有 `~$<檔名>` 鎖檔）一律跳過。
- 先寫到 `.<檔名>.n2p-partial`，完成才改名：失敗或取消不會毀掉既有檔案。
- 取消會在目前這頁做完後停止（每頁數秒到數十秒）。
- 每份做完會釋放 LaMa／SAM，下一份再重新載入（GPU 版有編譯快取，重載約 0.2 秒）。
- 「進階設定」可看模型狀態並下載缺少的模型。
- 開發用的環境變數（預載檔案、自動開始、只截本視窗）見 `crates/n2p-gui/src/debug.rs`。

### 打包成 macOS App

```bash
./scripts/package-macos-app.sh --zip                 # ad-hoc 簽章，只適合本機
./scripts/package-macos-app.sh --developer-id        # Developer ID + Hardened Runtime
./scripts/notarize-macos.sh                          # 公證、釘上票證，寫出 dist/NBLM2PPTX-notarized.zip
./scripts/package-macos-app.sh --app-store           # Mac App Store 版（沙盒），寫出 dist/NBLM2PPTX.pkg
```

- 產物：`dist/NBLM2PPTX.app`（bundle id `com.yikai.nblm2pptx`；0.1.1 以前是 NotebookLM2PPT.app／
  `com.yikai.notebooklm2ppt`），只含圖形介面，不含 `n2p` 命令列。
- Mac App Store 版的差異、沙盒實測與上架流程見 [`docs/APP_STORE.md`](docs/APP_STORE.md)。
- 圖示原稿是 `macos/AppIcon.svg`，改完執行 `./scripts/make-icon.sh` 重新產生 `AppIcon.icns`。
- 執行檔 strip 後約 29 MB，zip 約 12 MB；ONNX Runtime 已靜態連結，只依賴 macOS 內建框架。
- 模型（約 253 MB）不包進 app：第一次開啟時介面最上方會出現「首次使用：下載模型」，
  下載到 `~/Library/Caches/n2p/`（與命令列共用）。系統清除快取後會再次提示下載。
- 只支援 Apple Silicon。LaMa 的 GPU 路徑需要 macOS 12 以上（11 預期退回 CPU，未實測）。
- 公證憑證只從環境變數讀（`APP_STORE_CONNECT_API_KEY_PATH`／`_KEY_ID`／`_ISSUER_ID`，
  或 `NOTARYTOOL_PROFILE`）；`.p8` 不要放進 repo。
- 轉換期間以 `NSActivityUserInitiated` 關閉 App Nap：視窗在背景時，沒有這個設定會慢約 3 倍
  （CH1 第 8 頁實測 234 秒對 70 秒），並防止系統在轉換中閒置睡眠。

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

# 對既有檔案做基準回歸（基準在 fixtures/compat/manifest.json，PowerPoint 16.101 錄製；
# 升版後內容不同時，確認無誤再加 --record 重錄）
cargo run -p xtask -- compat --dir fixtures/probe-pptx

# 像素級視覺回歸（需要 LibreOffice）
cargo run -p xtask -- visual-diff --dir fixtures/probe-pptx --record
cargo run -p xtask -- visual-diff --dir fixtures/probe-pptx
```

`visual-diff` 的基準圖在 `fixtures/visual/`，旁邊的 `meta.json` 記著產生它的
LibreOffice 版本與 DPI；環境不符時會先警告再比對。實測 LibreOffice 的渲染逐位元可重現
（25 頁平均差全部 0.00），所以任何非零差異都是真的改到了東西。

`compat` 只會關閉自己開啟的文件，永不碰使用者在 PowerPoint 裡的其他簡報，
也不會呼叫 `quit`。macOS 上 PowerPoint 的 AppleScript 有若干陷阱
（拿不到畫布高度、無匯出 PDF 命令、集合迭代會卡死、相對路徑會彈 modal），
全部記錄在 [`docs/04-powerpoint-automation.md`](docs/04-powerpoint-automation.md)。

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
| `n2p-cli` | `n2p` 命令 | 取代 `cli.py` 的命令列 |
| `n2p-gui` | `n2p-gui` 圖形介面 | 取代 tkinter GUI |

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
| **LaMa**（`--erase lama`／有模型時的 `auto`） | 木紋、照片、複雜紋理 | ONNX Runtime 本機推論（macOS 預設 GPU），512×512 圖塊 + 羽化加權混合；遮罩只擴 3–6 px |

平坦／有紋理的判斷用文字外圍一圈的**亮度中位數絕對偏差**（MAD < 6 視為平坦）。
用 MAD 而不是標準差，是因為環上常夾著裝飾（引號、標靶圖案），標準差會被那幾個
極端值撐大。

LaMa 模型（Apache-2.0，約 199 MiB）不進版控，第一次使用前抓一次：

```bash
n2p models fetch     # 下載到 ~/Library/Caches/n2p/lama_fp32.onnx
n2p models status    # 檢查是否就緒
```

沒有模型時 `--erase auto` 會自動退回只擦筆畫的插值模式，不會失敗。

### LaMa 推論裝置（GPU／CPU）

macOS 上 LaMa 預設在 GPU（CoreML，Metal）推論。原始模型不能直接交給 CoreML：
它的 FFT 用 `Einsum` 寫成，還有 5 維的 `Gather`、`Neg`、帶 `output_padding` 的
`ConvTranspose`，大半節點會退回 CPU，反而比純 CPU 慢。所以第一次載入時
（或 `n2p models fetch` 之後）會在原模型旁產生改寫版 `lama_fp32.coreml-v1.onnx`
（約 1 秒、多佔 209 MB 磁碟），步驟見 `n2p_image::gpu`：

1. batch 維度固定為 1，交給 ONNX Runtime 做常數摺疊（17,480 → 2,841 個節點）。
2. `Einsum(ij,jk->ik)` → `MatMul`；`Neg` → `Mul(-1)`；純量 `Gather` → `Slice` + `Squeeze`；
   `ConvTranspose` 改成不裁切的完整輸出再用 `Slice` 裁切。全部是數學等價改寫，有單元測試逐元素比對。

改寫後 2,916 個節點全部落在 CoreML 的單一分區。CoreML 編譯快取放在
`~/Library/Caches/n2p/coreml/v1/`（第二次載入由約 2 秒降到 0.2 秒）。

| 實測（M 系列，`--high-res-background`） | 純 CPU | GPU |
| :--- | ---: | ---: |
| 每個 512 圖塊 | 1.1 秒 | 0.20 秒 |
| CH1 第 8 頁（約 28 塊） | 37.9 秒 | 8.6 秒 |
| CH4 第 1–3 頁 | 19.0 秒 | 8.4 秒 |

GPU 與 CPU 的輸出每通道最多差 1（0–255），肉眼無法分辨。

- `N2P_LAMA_DEVICE=gpu|cpu|auto`（預設 `auto`：GPU 載入失敗就退回 CPU；`gpu` 失敗直接報錯）。
- 改寫版用的是 CoreML ML Program，需要 macOS 12 以上；macOS 11 上 GPU 載入應會失敗而退回 CPU（未在 macOS 11 實測）。
- 所有 LaMa session 都關掉 `ort` 的自動裝置選擇（`AutoDevicePolicy::PreferCPU`）。
  `ort` 在 macOS 預設會自動掛上 NeuralNetwork 格式的 CoreML，把原始模型切成
  CPU／CoreML 交錯的碎片：比純 CPU 慢約 2.5 倍、載入多約 11 秒，精度也只剩 fp16。
  2026-10-21 以前的輸出就是這條混合路徑產生的，和現在的 CPU 輸出有極小差異
  （CH1 第 1 頁 0.01% 像素差超過 1%）。

## 物件切出（`--extract-objects`，實驗性）

擦完文字之後，把背景上的獨立物件（圖示、徽章、對話框、外框）切成帶透明度的 PNG，
放在背景之上、文字之下，可以在 PowerPoint 裡移動與編輯。預設關閉。

| 背景 | 偵測 | 從背景擦掉物件 |
| :--- | :--- | :--- |
| 平坦底色（底色像素 ≥ 35%） | 與底色差距 > 18 的連通區塊（`n2p_image::objects`），免模型、確定性 | 遮罩外擴 6 px，直接填底色 |
| 照片、漸層 | MobileSAM 自動分割（`n2p_image::sam`），16×16 格點 | 遮罩外擴頁寬的 1.2%，交給 LaMa |

SAM 的後處理：

- IoU ≥ 0.88、穩定度 ≥ 0.92；面積 0.05%–25%；外接框不超過頁面 80%；貼頁緣的丟掉。
- 每個遮罩只留最大連通塊（SAM 常夾帶相鄰物件的碎片）。
- 有一半以上落在已擦字區域的丟掉（文字塗抹塊）。
- 碎片合併：碎片彼此相接 → 以整體取代；碎片之間有底色縫隙 → 丟掉合併遮罩。
  IoU 0.70 起的低信心候選只能用來把兩個以上的碎片合成整體（雪花）。

SAM 模型（MIT，約 43 MiB）同樣由 `n2p models fetch` 下載。沒有模型時，非平坦背景的
頁面不切物件。

限制：

- 物件上的文字是獨立文字框，移動物件時文字不會跟著動。
- 物件移走後露出的是補出來的背景（平坦頁填底色；其他頁是 LaMa 生成，可能有暗塊）。
- 與其他圖形相連的小物件可能切不出來（CH1 第 4 頁的徽章 3）。
- SAM 只在 CPU 上跑（ort 自動掛的 CoreML 慢 3.4 倍、峰值多 4 GB；MLProgram 格式建不出執行計畫）。
  CH1（11 頁，LaMa 用 GPU）實測 86 秒，標準版 45 秒。
- 除錯：`examples/sam_debug.rs` 輸出彩色疊圖；`N2P_OBJECT_GROW`、`N2P_EXCLUDE_GROW`
  可覆寫兩個外擴量。

## 已知限制

1. **Vision 不給語意標籤。** 所有文字都建成內文框，不會自動判定「這是標題」。需要 `paragraph_title`
   這類語意時請走 `--layout mineru`。
2. **Vision 不做圖片區塊偵測。** 預設圖片留在背景圖上不被擦除；視覺結果與 Windows 版一致
   （Windows 版也是「文字變文字框、其餘留背景」）。需要可移動的物件時用 `--extract-objects`。
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

規格文件在 [`docs/`](docs/)。
