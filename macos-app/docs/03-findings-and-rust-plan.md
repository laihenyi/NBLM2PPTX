# 關鍵發現與對 Rust 改寫的影響

---

## 發現 1（最關鍵）：`clean_ppt` 不是清理，是去水印

repo 裡這段看起來像「順手清理多餘形狀」：

```python
# notebooklm2ppt/utils/ppt_combiner.py
def clean_ppt(in_ppt_file, out_ppt_file):
    ppt = PptxPresentation(in_ppt_file)
    for slide in ppt.slides:
        for shape in list(slide.shapes):
            if shape.name == "New shape":
                sp = slide.shapes._spTree.remove(shape._element)
    ppt.save(out_ppt_file)
```

實測 `spire.presentation 11.8.1` 未授權執行時，**存檔階段會對每一頁附加**一個名為 `"New shape"` 的形狀，內容是：

> `Evaluation Warning : The document was created with Spire.Presentation for Python`

驗證：3 份單頁 PPTX 合併後，3 頁各帶 1 個水印；`clean_ppt` 精準刪掉 3 個，其餘 `Block_text` 形狀全部保留。

**結論：**

- 這個專案目前跑在 Spire 評估模式，靠事後刪形狀繞過水印。這是既有事實，不是猜測。
- **Rust 版完全不需要這個邏輯**：自己產生 XML 就不會有水印，`clean_ppt` 整個函式與相關呼叫可以刪除。
- 這也意味著**目前的輸出在「有無授權」下有兩條路徑**：授權版沒有水印，`clean_ppt` 是空操作；評估版靠刪除。Rust 版把兩條路徑統一成一條。
- 附帶風險：任何形狀若未命名，Spire 預設叫 `"New shape"`，會被連帶刪掉。`ppt_creater.py` 因為有 `shape.Name = f"Block_{label}"` 才沒事。Rust 版自然免疫。

---

## 發現 2：`AppendBySlide` 是盲目 part 匯入，3 份同模板 → 3 套 master

實測 3 份同構單頁 PPTX 合併：

| part | Spire 產出 | 理論最小 | 膨脹 |
| :--- | ---: | ---: | ---: |
| slideMaster | 3 | 1 | 3× |
| slideLayout | 33 | 11 | 3× |
| theme | 3 | 1 | 3× |
| 檔案大小 | 74,687 B | ≈ 30,000 B | ≈ 2.5× |

Spire 不判斷來源 master 與目標 master 是否等價，一律整套匯入。

**對 Rust 改寫的影響：這是可以「做得比原版更好」的地方，而不是必須複製的行為。**

只要來源同構（本專案必然如此），合併時只搬 `slideN.xml` + 其 media，layout 全部共用基底那一套。預期檔案小 60%、PowerPoint 開啟更快、`refine_ppt` 的走訪也更單純（只有一組 layout 要處理）。

失敗條件：來源的 `slideLayout7.xml` 雜湊不同（= 不同模板）。此時 fallback 到完整 part 匯入。實作上就是「合併前先比對 layout 雜湊，不一致才走完整匯入」。

---

## 發現 3：`FitTextToShape = True` 在 Spire 11.8.1 是空操作

`ppt_creater.py` 的 `create_text_shape` 有這行：

```python
text_shape.TextFrame.FitTextToShape = True
```

但實際輸出的 `<a:bodyPr>` 只有：

```xml
<a:bodyPr lIns="0" tIns="0" rIns="0" bIns="0" rtlCol="0" anchor="ctr"/>
```

**沒有** `<a:normAutofit/>`、`<a:spAutoFit/>` 或 `<a:noAutofit/>`。

意思是：目前產出的 PPTX 依賴 PowerPoint 的預設行為，而不是「縮放文字以符合圖形」。這解釋了為什麼 MinerU 重排版後的文字偶爾會溢出框外。

**對 Rust 改寫的影響：**

- 完全等價 = 也不寫 autofit。
- 但這是一個明確的**改善機會**：`<a:normAutofit fontScale="..." lnSpcReduction="..."/>` 能讓 PowerPoint 自動縮字。若要改，必須在 golden fixture 上確認視覺差異可接受，並列為「刻意的行為變更」而非「移植 bug」。

---

## 發現 4：`anchor="ctr"` 是 Spire 的預設，不是程式碼指定的

repo 沒有設 `TextFrame.Anchor`，但輸出仍是 `anchor="ctr"`（垂直居中）。Spire 建立新形狀時預設就是居中。

同理 `lang="en-US"`、`dirty="1"`、`<p:style>` 區塊也都是 Spire 的預設值，不是程式碼要求。

**對 Rust 改寫的影響：** 這些預設值要明確認列為「Spire 預設」，決定抄或不抄。建議抄（保留 `anchor="ctr"`、`dirty="1"`），因為既有使用者的版面是照這個結果調的。

---

## 發現 5：`docProps/app.xml` 有兩個 Spire 自身的 bug，不要複製

```xml
<Application>Spire.Presentation for .NET 2.1.0.0</Application>
<PresentationFormat>全屏显示(4:3)</PresentationFormat>
<Slides>1</Slides>
```

- `Application` 寫 `2.1.0.0`，但 `presentation.xml` 的註解寫 `11.8.0.0` —— 版本號不一致。
- `PresentationFormat` 對於 16:9 的畫布仍寫「全屏显示(4:3)」—— 這是錯的，會讓部分工具誤判版面比例。
- `Slides` 在 3 頁合併檔裡仍寫 `1` —— 沒有更新。

**對 Rust 改寫的影響：** 這裡應該寫對。`PresentationFormat` 依實際 `sldSz` 決定，`Slides` 寫實際頁數。這是低風險的正確性修正。

---

## 發現 6：`spire.presentation` 可在 macOS 執行，不限 Windows

實測 macOS arm64 直接可用，不需 .NET runtime、不需 PowerPoint：

```bash
python3 -m venv /tmp/spire_venv
/tmp/spire_venv/bin/pip install spire_presentation-11.8.1-py3-none-macosx_11_0_arm64.whl
/tmp/spire_venv/bin/python -c "import spire.presentation as sp; print(sp.Presentation().Slides.Count)"
```

**對 Rust 改寫的影響（最重要的一條）：** 這代表 **`n2p-ooxml` 不必在 Windows 上開發或驗證**。只要把「產生參考 PPTX」與「比對 XML」兩件事自動化，整個 OOXML 引擎（佔總工作量 40% 的最大單項）可以在 macOS／Linux 上完成並用 CI 驗證。這把 Phase 2 的風險大幅降低。

---

## 發現 7：引擎表面有 60% 是本專案用不到的

3,927 個匯出符號裡，`Chart*` 相關超過 600 個，`Audio`/`Video`/`OleObject`/`SmartArt` 再加數百個。本專案實際需要的只有 10 個型別、約 400 個匯出。

**對 Rust 改寫的影響：** 不要被「Spire 有 48 MB」嚇到。要重寫的不是那個引擎，而是一個約 400 函式等價、且只涵蓋 PPTX 寫入子集的迷你引擎。粗估 3,000–4,500 行 Rust（含 OPC 封裝與 rels 管理）。

---

## 對 Phase 2（OOXML 引擎）的修正建議

原本估 10–15 人日，根據以上發現可以重估並具體化：

| 工作項 | 估計 | 驗證方式 |
| :--- | ---: | :--- |
| OPC 封裝（zip + `[Content_Types]` + `_rels` 讀寫） | 2–3 天 | 往返讀寫 Spire 產出的基準檔，XML 位元相同 |
| `presentation.xml` / `sldSz` / `sldIdLst` 維護 | 1 天 | 對照 `../fixtures/probe-pptx/sz_*.pptx` |
| 文字框（`p:sp` + `a:txBody`） | 2 天 | 對照 `1_text.pptx`、`dump/1_text/ppt/slides/slide1.xml` |
| 背景圖片（`p:bg` + media + rels） | 1.5 天 | 對照 `2_background.pptx` |
| 插圖置底（`p:pic` + ZOrder） | 1.5 天 | 對照 `3_picture.pptx` |
| 合併（同構捷徑 + 完整匯入 fallback） | 3–4 天 | 對照 `4_merged.pptx`；並驗證輸出比它小且 PowerPoint 可開 |
| 形狀走訪／刪除／換字型 | 2 天 | 對照 `5_cleaned.pptx` |
| LibreOffice 視覺回歸 harness | 1–2 天 | 全部產出轉 PDF → PNG → 像素比對 |

合計 **14–17 天**，與原估計的 10–15 天上限略增，但**風險等級從「高」降到「中」**，因為每一項都有可逐位元比對的參考輸出。

---

## 一句話總結

這個引擎不需要「反編譯」，需要的是**把它當成規格產生器來用**：跑一次、拆一次 zip、把 XML 抄成模板。3,927 個匯出符號告訴你邊界在哪（只要 400 個），輸出檔案告訴你要寫什麼。而 Rust 版在合併與水印兩件事上可以直接做得比原版好，不必背著它的歷史包袱。

---

## 發現 8：Spire 把多行文字塞進單一 `a:t`，不拆段落

`TextFrame.Text = "line one\nline two"` 的實際輸出是**單一段落、單一 run**，
換行字元原樣留在 `<a:t>` 裡：

```xml
<a:r><a:rPr lang="en-US" dirty="1"/><a:t>line one
line two</a:t></a:r>
```

樣本：`../fixtures/probe-pptx/6_newline.pptx`。

**對 Rust 改寫的影響：** 這是規格上不正確但實務上可用的寫法。`n2p-ooxml` 預設改用
`<a:br/>`（規格正確、視覺相同），並保留 `NewlineMode::Literal` 供回歸比對。
`ppt_creater` 的 MinerU 文字區塊常含 `\n`，切換前需視覺比對。

---

## 平台修正：目標是 macOS，不是 Windows

原始分析假設要移植 Windows 自動化層（pywin32／COM／模擬滑鼠）。使用者確認**目標平台是 macOS**，
因此那整層不只需要重寫，而是**根本不需要**：

| 原計畫 | 修正後 |
| :--- | :--- |
| `n2p-win`（Win32 螢幕／輸入／視窗列舉／COM） | **刪除** |
| 依賴微軟電腦管家「智慧圈選」取得 OCR | 改用 macOS 內建 **Vision**（`VNRecognizeTextRequest`） |
| 逐頁截圖 → 合併單頁 PPTX | 直接由 `n2p-build` 產生整份簡報，不需要合併 |
| `n2p-gui`（egui／slint） | 先做 CLI（`n2p`），GUI 之後再說 |

這個修正讓整體風險顯著下降：原本最脆弱、最無法在 macOS 驗證的部分（視窗行為耦合）
直接消失，取而代之的是一段確定的 API 呼叫。

Vision OCR 實測（`fixtures/floyd.pdf` 第 1 頁）：

```
辨識 3 筆：
  [0] conf=0.50 bbox=(0.129,0.418,0.742,0.097) "穿透表象：Floyd 算法的动态规划之魂"
  [1] conf=0.50 bbox=(0.308,0.530,0.383,0.052) "一次从01 背包到空间优化的深度漫游"
  [2] conf=0.30 bbox=(0.923,0.971,0.071,0.016) "今NotebookLM"
```

端到端輸出：2 頁 16:9、背景為原頁面影像、7 個可編輯文字框，中文內容與位置正確。

---

## 完成狀態

| 階段 | 內容 | 狀態 |
| :--- | :--- | :--- |
| Phase 0 | golden fixtures（Spire 輸出、PyMuPDF 輸出、numpy 輸出） | ✅ |
| Phase 1 | `n2p-core`、`n2p-pdf`、`n2p-image`、`n2p-layout`、`n2p-mineru` | ✅ |
| Phase 2 | `n2p-ooxml`：OPC、XML DOM、`p:sp`、`p:pic`、`p:bg`、merge、props | ✅ |
| Phase 3 | ~~`n2p-win`~~ → `n2p-vision`（Vision OCR） | ✅ |
| Phase 4 | `n2p-build`、`n2p-pipeline`、`n2p-cli` | ✅ |
| Phase 5 | GUI | ⬜ 未開始（CLI 已可用） |
| Phase 6 | 打包、CI | ⬜ 未開始 |

規模：10 個 crate、**178 個測試全綠**、`cargo clippy --all-targets` 零警告。

### 各階段驗證結果

| 項目 | 驗證方式 | 結果 |
| :--- | :--- | :--- |
| `p:sp`／`p:pic`／`p:bg` | 與 Spire 11.8.1 實際輸出做 DOM 比對 | 逐節點、逐屬性相同 |
| 合併 | 3 份同模板 → 對照 `4_merged.pptx` | 1 master / 11 layouts / 1 theme、30,677 bytes（Spire 74,295） |
| 合併 fallback | 人工改動 layout 觸發非同構 | 正確匯入 2 master / 22 layouts / 2 themes |
| PDF 幾何 | 與 PyMuPDF 比對 | 頁數、頁面尺寸、像素尺寸（2867×1600）完全相同 |
| PDF 像素 | 與 PyMuPDF 逐像素比對 | 平均差 0.316／255、僅 0.5% 像素差 > 8（純抗鋸齒） |
| 去水印 4 法 | 與 numpy 實作逐像素比對 | **0 個不同像素** |
| 邊緣色彩統計 | 同上 | diversity 與填色完全相同（0.436363…、`[241,243,248]`） |
| Vision OCR | 真實頁面 | 中文標題正確辨識 |
| PPTX 可讀性 | `python-pptx` 交叉驗證 | 畫布、背景填滿、文字位置與內容皆正確 |

### 實測：真實 NotebookLM 簡報（13 頁）

`CH9_用 2.0 版的愛打造正向課堂.pdf`（1376×768 pt × 13 頁）：

```
n2p convert ... --high-res-background
  13 頁、84 個文字框、去水印 13 頁、7.3 MB、5.4 秒
```

用 LibreOffice 逐頁渲染檢視，發現並修掉一個真實缺陷：

| 問題 | 原因 | 修法 |
| :--- | :--- | :--- |
| 文字壓在藍色圓角框上時，擦除留下白色方塊 | 擦除是「整塊填成邊緣主色」，該區塊邊緣以白色雲朵為主 → 填白 | 改用 `n2p-image` 的修補引擎從四周插值（`--erase inpaint`，新預設） |
| 標題下方的黃色同心圓被抹成灰色 | 同上 | 同上，同心圓現在完整保留 |

`--erase fill` 保留原本行為以便對照。插值版本的成本是 3.6 s → 5.4 s（13 頁），可接受。

仍未解決：少數短筆畫（引號、破折號）會殘留在背景上，因為 Vision 沒把它們辨識成獨立區塊。

### 字級不準：從 −35% 修到 ±4%

使用者回報「文字框辨識成功，但字級與原稿不同，事後要反覆調整」。量測後確認是三個獨立缺陷疊加：

**1. 公式方向錯了。** Python 版是 `font_size = bbox高度 × 0.75`。對 MinerU 那種「區塊框含行距」的座標勉強可用，但 Vision 的框是貼齊墨跡的，等於先假設墨跡只有 0.75 em，又再乘一次 0.75 —— 系統性偏小 30% 以上。

改用**字寬**：CJK 等寬全形，一個字就是一個 em，所以 `em ≈ 行寬 / 全形字數`。實測同一段文字的兩行推得 42.0 與 42.7 pt（誤差 1.7%）。墨跡高度只當備援，因為同一字級下 `一二三` 與 `提學國` 的墨跡差到 40%。

**2. 文字框沒有跟著長大。** bbox 貼齊墨跡，而 PowerPoint 會在第一行上方加行距、最後一行下方加 descender。字級修準之後，兩行字反而塞不進兩行的墨跡框，於是溢出、甚至被折行。改成依「字級 × 行數」重算框，並寫 `wrap="none"`（原稿每個視覺行就是一行）。

**3. 分群把不同字級的行併在一起。** 判斷條件原本只看「間距 ≤ 較大行高 × 0.5」，於是 0.185 高的大標題行把下方 0.062 高的小字吸進來，整段用 88 pt 渲染。加上「兩行高度比 ≤ 1.5」後解決。

另外把 `<a:ea>` 補上：CJK 字元實際用的是東亞字型，只寫 `<a:latin>` 會落到主題字型，字高再差一成。

驗證（第 1 頁，比較原圖與輸出的 OCR 墨跡框）：

| 文字 | 寬度誤差 | 高度誤差 |
| :--- | ---: | ---: |
| 用2.0版的愛， | 0.0% | −1.8% |
| 好好愛他們！ | +3.1% | 0.0% |
| 提升學生正向幸福 | −1.6% | −6.7% |
| 創造共享的溫暖 | +0.9% | −9.7% |
| 感的課堂領導力 | −4.1% | −21% |
| 與連結瞬間 | +3.2% | +9.5% |

寬度全部在 ±4% 內。高度仍有個位數到兩成的差異，來源是字型本身的差異（原稿的字型不明，只能用系統字型近似），不是字級算錯。

### 字型：Noto Sans TC

NotebookLM（現在的 Gemini Notebook）產出的簡報是**圖片**，字型被畫進像素裡，PDF 裡
沒有任何字型屬性可讀（實測 `page.get_text("dict")` 回傳 0 個 span）。所以只能用外觀推測。

查證後採用 **Noto Sans TC（思源黑體）**：

* Google 的產出物，社群處理繁中 NotebookLM 簡報時的共識也是 Noto Sans TC／思源黑體
  （[就享知](https://www.digiknow.com.tw/knowledge/19f7ff4acd944)、
  [小企鵝 Penchan](https://penchan.co/ai/presentation/ai-presentation-prompt-tips/)）。
* Google Slides 的 zh-Hant 標準字型方案就是 Noto Sans TC
  （[OpenPresentation](https://www.openpresentation.org/catalogs/font-schemes/noto-sans-tc)）。
* Noto Sans CJK TC 含完整拉丁字形，整份簡報可以只用一個字型。

實作上不寫死名字。第一版寫死 `Microsoft JhengHei`，但本機根本沒裝 —— PowerPoint 會
**靜默**落到主題字型，等於沒設定。改成偵測標準字型目錄並依偏好序挑選：

```
$ n2p fonts
  ✓ 已安裝  Noto Sans TC  ← 將使用
  · 未安裝  Source Han Sans TC
  · 未安裝  PingFang TC
  · 未安裝  Microsoft JhengHei
  ✓ 已安裝  Heiti TC
  · 未安裝  LiHei Pro
```

`system_profiler SPFontsDataType` 要 8 秒，不能每次轉換都跑；改成掃四個標準字型目錄的
檔名（一次 `read_dir`），對這幾個候選字型而言檔名是穩定的。

### 去字的關鍵：遮罩要對準筆畫，不是矩形

`NBLM2PPTX`（另一個實作，瀏覽器版）走的是完全不同的路：每頁呼叫兩次 Gemini ——

1. `gemini-2.5-flash-image`：「移除所有文字、保留背景」→ 由生成模型**重畫**一張無字背景
2. `gemini-2.5-flash(-lite)`：直接回傳 `font_size_pt`、`font_weight`、`text_align`、
   `color`、`line_height` 與 `box_2d`

它之所以沒有我們遇到的問題，是因為**去字是語意任務**：模型知道什麼是文字、什麼是圖示，
所以子彈點、光暈、漸層都自然保留。

順著這個思路看，才發現我們的根本錯誤不在演算法，而在**遮罩的形狀**：

> 一直用「文字區塊的矩形」當遮罩，所以同一區域裡的子彈點、光暈、圖示必然被抹掉，
> 只好再寫 `trim_leading_icon`、`is_decoration_block`、墨跡投影找空隙……一整層幾何啟發式
> 去補救。那條路沒有終點。

改成**逐像素遮罩**：只有「顏色接近取樣文字色」的像素才進遮罩。

* 子彈點是黃底深藍環 → 只有環是文字色，黃色保留（實測第 7 頁三個子彈點完整）
* 標題的白色外框光暈 → 不是文字色，完整保留，新文字剛好坐在原本的光暈裡（比擦掉更接近原稿）
* 引號裝飾 → 不會變成 "99" 重複文字

代價只有兩項，都可接受：需要先正確取樣文字色（背景要從 bbox **外圍一圈**取，
不能取框內 —— 密集段落裡文字面積大於空白）；遮罩要膨脹 1–3 px 吃掉反鋸齒邊緣
（否則殘影 + 背景 PNG 從 0.6 MB 漲到 1.6 MB/張）。

**這不需要 API、不需要本地模型。** 他們用生成模型取得語意；我們用「文字色」這個
便宜的先驗（prior）達到同樣的保護效果。

仍然做不到的是：文字壓在**照片**上時，我們只能局部插值，他們能生成合理的背景內容。
NotebookLM 的簡報背景是色塊與插圖，這條差異目前不影響結果。

另一個他們有、我們沒有的：粗體／斜體／對齊／行距。Vision 不提供，我們只能從像素推
顏色與大小。要補這塊可以加 `--ocr gemini`，但會引入 API 依賴。

### 未完成的部分與理由

* **GUI**：CLI 已能完成全部工作，GUI 屬於體驗層，等核心穩定再做較合理。
* **`griddata`／`skimage` 修補**：Rust 生態沒有 SciPy `griddata` 與 `inpaint_biharmonic` 的等價物，
  已改用 IDW 與調和插值，並在 `n2p methods` 明確標示「替代演算法，視覺接近」。
* **視覺回歸 harness**：本機沒有 LibreOffice，改以 `python-pptx` 做結構與座標驗證。
  要鎖住「像素級視覺相同」仍需在 CI 上裝 LibreOffice。
* **Windows 版相容**：已放棄。若日後需要雙平台，Windows 側應改走 `--layout mineru`，
  而不是把截圖自動化搬回來。

## 遮罩：從「調參數」改成「用教科書方法」

前幾輪的作法是「矩形平塗 → 插值 → 逐像素筆畫遮罩」，其中筆畫遮罩靠一整層手工規則
（顏色距離、亮度極性、彩度門檻、`trim_leading_icon`、`is_decoration_block`…）。
每一條規則都在猜同一件事：**這塊區域裡哪些像素是字**。

查了同類專案之後發現 `Appt-OCR`（PaddleOCR + LaMa + python-pptx）解的是同一個問題，
而它的遮罩方法是標準影像處理：

| | 手工規則版 | Otsu ∪ 自適應版 |
| :--- | :--- | :--- |
| 極性 | 取樣文字色再比亮度 | **框線平均亮度**（不會被光暈帶偏） |
| 門檻 | 單一固定值（調了八次） | **全域 Otsu ∪ 局部自適應**，兩者互補 |
| 補斷筆 | 膨脹 | **形態學閉運算** |
| 文字色 | 離背景最遠的色 | Otsu 遮罩內像素平均 |
| 字重 | 無 | 遮罩密度 > 0.25（Vision 拿不到字重，這個免費） |

換掉之後 `trim_leading_icon`、`is_decoration_block` 之外的手工規則全部刪除。

### 只有一個參數不能照抄：閉運算核

Appt-OCR 的核是 `單行框高 / 25`。照抄成「區塊高度 / 25」會在多行標題上炸掉：
CH7 第 1 頁的三行標題區塊高 340 px → 核 13 px，比筆畫間隙還大，閉運算把整個標題
糊成幾塊黑色板子。改成用 **em 估算**（`em/25`，上限 4 px）後正常。

### 去字：LaMa 補紋理，插值補平坦

LaMa（WACV 2022，Apache-2.0）透過 ONNX Runtime 在本機跑，模型 199 MiB，
輸入 `[1,3,512,512]`（0–1）、`mask [1,1,512,512]`（0/1），輸出 0–255，空間維度固定 512，
所以要切 512 圖塊並羽化加權混合，CPU 約 2.7 s/塊。

實測三件事跟前述直覺相反：

1. **遮罩不能為 0 擴張。** 只擦筆畫會留下抗鋸齒亮邊，LaMa 補完之後那些亮邊正好
   框出字的輪廓，看起來像一層鬼影（`stroke` 插值反而沒這問題，因為它從四周擴散）。
2. **遮罩也不能大擴張。** 連光暈一起擦（0.25–0.4 字高）時，LaMa 只能憑空補一大塊，
   結果是比周圍亮的模糊色塊，比原本的光暈更明顯。
3. **平坦背景不該用 LaMa。** 大片純色上它會補出偏暗的雲狀色塊；插值填色乾淨得多。

定案：遮罩擴 `3–6 px`（吃亮邊）＋ 依背景平坦度分工 —— 平坦（MAD < 6）用插值並
擴到 `0.30 × 字高` 把光暈一起補掉；有紋理用 LaMa 並留著光暈（它剛好成為新文字的外光）。

平坦度用**中位數絕對偏差**而非標準差：環上常夾著引號、標靶圖案，標準差會被撐大到
把「平坦藍底」誤判成有紋理。

### 一個真正的 bug：羽化權重為 0 會留下原字

圖塊邊緣的羽化權重原本夾在 `0..1`。若某個遮罩像素只被單一圖塊覆蓋（區域邊界就是這種），
權重 0 → 累加時被跳過 → **原稿文字原封不動留著**。改成下限 0.02，並讓圖塊起點往外挪
半個重疊，使區域邊界落在圖塊內部。

### 結果

| 簡報 | 頁數 | 文字框 | 檔案 | 耗時（LaMa） |
| :--- | ---: | ---: | ---: | ---: |
| CH7（深色攝影背景 + 白字光暈） | 15 | 115 | 23.9 MB | 63 s |
| CH9（平坦色塊 + 深藍字柔白外光） | 13 | 88 | 13.2 MB | 69 s |

插值模式仍是 13 頁約 5 秒；LaMa 的代價是每頁多 3–5 秒（視文字區塊數與面積）。
