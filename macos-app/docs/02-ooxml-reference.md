# OOXML 規格：本專案需要的 8 個操作

> 全部由 Spire.Presentation 11.8.1 實際執行後輸出、拆包、逐節點比對得出。
> 環境：macOS arm64、`spire_presentation 11.8.1`、`FileFormat.Pptx2016`。

## 單位換算（先記住，後面都靠它）

| 概念 | 換算 | 例 |
| :--- | :--- | :--- |
| 點（pt）→ EMU | × 12700 | `60 pt` → `762000` |
| 點（pt）→ 字級 centipoint | × 100 | `24.0` → `sz="2400"` |
| 預設 4:3 畫布 | 9144000 × 6858000 EMU（720 × 540 pt） | — |

Spire 的 `RectangleF.FromLTRB(l, t, r, b)` 傳入點，寫出時無條件 × 12700 並轉整數：

```
FromLTRB(60, 40, 660, 120)
→ <a:off x="762000" y="508000"/>  <a:ext cx="7620000" cy="1016000"/>
   (660-60)*12700 = 7620000      (120-40)*12700 = 1016000
```

---

## 1. 畫布尺寸（`setup_presentation`）

寫在 `ppt/presentation.xml`，**單一屬性即可**：

```xml
<!-- SlideSizeType.Screen16x9  -->
<p:sldSz cx="12192000" cy="6858000" type="screen16x9"/>
<!-- SlideSizeType.Screen16x10 -->  ← 注意是 10in × 6.25in，不是 13.33×8.33
<p:sldSz cx="9144000"  cy="5715000" type="screen16x10"/>
<!-- SlideSizeType.Screen4x3   -->
<p:sldSz cx="9144000"  cy="6858000" type="screen4x3"/>
```

Rust 對照：

```rust
fn sld_sz(pt: f64) -> (i64, i64) { ((pt * 12700.0) as i64, (pt * 12700.0) as i64) }
// 16:9  → 13.333in × 7.5in  = 12192000 × 6858000
// 16:10 → 10in    × 6.25in  =  9144000 × 5715000
// 4:3   → 10in    × 7.5in   =  9144000 × 6858000
// custom→ w*12700, h*12700
```

---

## 2. 文字框（`create_text_shape`）

寫在 `ppt/slides/slideN.xml` 的 `<p:spTree>` 內，附加在 `grpSpPr` 之後：

```xml
<p:sp>
  <p:nvSpPr>
    <p:cNvPr id="2" name="Block_text"/>
    <p:cNvSpPr/>
    <p:nvPr/>
  </p:nvSpPr>
  <p:spPr>
    <a:xfrm>
      <a:off x="762000" y="508000"/>
      <a:ext cx="7620000" cy="1016000"/>
    </a:xfrm>
    <a:prstGeom prst="rect"/>
    <a:noFill/>
    <a:ln><a:noFill/></a:ln>
  </p:spPr>
  <p:style>
    <a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr></a:lnRef>
    <a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef>
    <a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef>
    <a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef>
  </p:style>
  <p:txBody>
    <a:bodyPr lIns="0" tIns="0" rIns="0" bIns="0" rtlCol="0" anchor="ctr"/>
    <a:lstStyle/>
    <a:p>
      <a:pPr algn="ctr"/>
      <a:r>
        <a:rPr lang="en-US" sz="2400" dirty="1">
          <a:solidFill><a:srgbClr val="000000"/></a:solidFill>
          <a:latin typeface="Calibri"/>
        </a:rPr>
        <a:t>Hello MinerU</a:t>
      </a:r>
    </a:p>
  </p:txBody>
</p:sp>
```

對應關係：

| repo 呼叫 | 產出的 XML |
| :--- | :--- |
| `shape.Name = "Block_text"` | `<p:cNvPr @name>` |
| `TextFrame.Text = ...` | `<a:p><a:r><a:t>`（單段單 run） |
| `TextFrame.MarginLeft/Right/Top/Bottom = 0` | `bodyPr` 的 `lIns/tIns/rIns/bIns="0"` |
| `TextFrame.FitTextToShape = True` | **無輸出**（Spire 未寫 `normAutofit`/`spAutoFit`） |
| `Line.FillType = none` | `<a:ln><a:noFill/></a:ln>` |
| `Fill.FillType = none` | `<a:noFill/>`（`spPr` 直下） |
| `paragraph.Alignment = Center` | `<a:pPr algn="ctr"/>` |
| `textRange.LatinFont = TextFont("Calibri")` | `<a:latin typeface="Calibri"/>` |
| `textRange.FontHeight = 24.0` | `<a:rPr sz="2400">` |
| `textRange.Fill.SolidColor = Color.FromArgb(255,0,0,0)` | `<a:solidFill><a:srgbClr val="000000"/></a:solidFill>` |
| 未指定錨點 | `anchor="ctr"`（Spire 預設居中） |

**兩個要注意的點：**

- `FitTextToShape = True` 在 Spire 11.8.1 **完全沒有寫進 XML**。這是既有行為，Rust 版若要完全等價就跟著不寫；若想改善（讓 PowerPoint 自動縮字），才加 `<a:normAutofit/>`。這會造成視覺差異，需在 golden fixture 上確認。
- `dirty="1"` 是「快取失效」標記，PowerPoint 會重算排版。保留它比較安全。
- `<p:style>` 區塊即使填色設為 none 仍然寫出。它不影響視覺（被 `a:noFill` 覆蓋），但省略可讓檔案小一點。建議保留以求等價。

---

## 3. 背景圖片（`process_slide_background`）

寫在 `<p:cSld>` 的第一個子節點 `spTree` 之前：

```xml
<p:bg>
  <p:bgPr>
    <a:blipFill dpi="0">
      <a:blip r:embed="rId2"/>
      <a:srcRect/>
      <a:stretch><a:fillRect/></a:stretch>
    </a:blipFill>
  </p:bgPr>
</p:bg>
```

`ppt/slides/_rels/slideN.xml.rels` 新增一條：

```xml
<Relationship Id="rId2"
              Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image"
              Target="../media/image1.png"/>
```

`[Content_Types].xml` 需有 `Default Extension="png"`（Spire 的基準檔本來就有）。

`FillType = Stretch` → `<a:stretch><a:fillRect/></a:stretch>`（若為 Tile 才會有 `a:tile`）。
`dpi="0"` 是 Spire 特有，PowerPoint 也接受，可省略。

---

## 4. 插圖並置底（`extract_foreground_element`）

`<p:pic>`，且因為 `ZOrderPosition = 0`，它被插在 `spTree` 中 **`grpSpPr` 之後的第一個位置**：

```xml
<p:pic>
  <p:nvPicPr>
    <p:cNvPr id="2" name="New picture"/>
    <p:cNvPicPr/>
    <p:nvPr/>
  </p:nvPicPr>
  <p:blipFill>
    <a:blip r:embed="rId2"/>
    <a:srcRect/>
    <a:stretch><a:fillRect/></a:stretch>
  </p:blipFill>
  <p:spPr>
    <a:xfrm>
      <a:off x="762000" y="508000"/>
      <a:ext cx="7620000" cy="1016000"/>
    </a:xfrm>
    <a:prstGeom prst="rect"/>
    <a:ln><a:noFill/></a:ln>
  </p:spPr>
</p:pic>
```

注意：`<p:pic>` 內是 `<p:blipFill>`，`<p:sp>` 內才是 `<p:txBody>`；兩者的 `spPr` 擺放位置也不同（`pic` 的 `spPr` 在 `blipFill` 之後）。

---

## 5. 刪除指定名稱的形狀（`clean_ppt`）

**這一項不是「清理多餘形狀」，是「去除 Spire 評估水印」。** 見 [`03-findings-and-rust-plan.md`](03-findings-and-rust-plan.md#發現-1最關鍵)。

Spire 未授權時，**存檔階段會對每一頁附加**一個形狀，長這樣：

```xml
<p:sp>
  <p:nvSpPr>
    <p:cNvPr id="3" name="New shape">
    ...
    <p:cNvSpPr><a:spLocks noSelect="1" noMove="1" noResize="1" noTextEdit="1"/></p:cNvSpPr>
  </p:nvSpPr>
  <p:spPr>
    <a:xfrm><a:off x="127000" y="254000"/><a:ext cx="6985000" cy="254000"/></a:xfrm>
    <a:prstGeom prst="rect"/><a:noFill/>
  </p:spPr>
  <p:txBody>
    <a:bodyPr rtlCol="0" anchor="ctr"/><a:lstStyle/>
    <a:p><a:pPr algn="ctr"/>
      <a:r><a:rPr lang="en-US" sz="1600" dirty="1">
        <a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>
      </a:rPr>
      <a:t>Evaluation Warning : The document was created with Spire.Presentation for Python</a:t></a:r>
    </a:p>
  </p:txBody>
</p:sp>
```

`clean_ppt` 的做法就是：走訪每頁 `spTree` 的所有 `<p:sp>`，刪掉 `p:cNvPr/@name == "New shape"` 的那些。

**Rust 版完全不需要這一步**——自己產生 XML 就不會產生水印。整個 `clean_ppt` 函式可以消失。

⚠️ 但要注意它的副作用：任何**使用者建立的形狀若未命名**，Spire 會預設叫 `"New shape"`，也會被一起刪掉。實測驗證：把來源形狀命名為 `Block_text` 後，`clean_ppt` 精準刪掉 3 個（3 頁各 1 個水印），內容形狀全數保留。這個雷在 Rust 版自然消失。

---

## 6. 合併（`combine_ppt_files_with_spire`）

### Spire 的實際行為：每個來源各帶一整套 master + layout + theme

實測：3 份同構單頁 PPTX 合併後，產出 `ppt/` 下有：

| part | 數量 |
| :--- | ---: |
| `slideMasters/slideMasterN.xml` | **3** |
| `slideLayouts/slideLayoutN.xml` | **33**（每份 11 個） |
| `theme/themeN.xml` | **3** |
| `slides/slideN.xml` | 3 |

而且每頁指向**不同**的一組：

```
slide1 → slideLayout7.xml   （→ slideMaster1）
slide2 → slideLayout18.xml  （→ slideMaster2）
slide3 → slideLayout29.xml  （→ slideMaster3）
```

`presentation.xml` 隨之長出三筆 master 與三筆 slide id：

```xml
<p:sldMasterIdLst>
  <p:sldMasterId r:id="rId1" id="2147483648"/>
  <p:sldMasterId r:id="rId2" id="2147483660"/>
  <p:sldMasterId r:id="rId9" id="2147483672"/>
</p:sldMasterIdLst>
<p:sldIdLst>
  <p:sldId r:id="rId7"  id="257"/>
  <p:sldId r:id="rId10" id="258"/>
  <p:sldId r:id="rId11" id="259"/>
</p:sldIdLst>
```

也就是說，`AppendBySlide` 做的是**盲目 part 匯入**：不判斷來源與目標的 master 是否等價。對本專案（所有來源同模板）來說，這是 3 倍膨脹。

### Rust 版可以做得更好

因為中間產物全部由同一個產生器（NotebookLM／截圖流程）產生，套用同構假設後只需要：

1. 取第一份作為基底（`slideMaster1` + 11 個 layout + `theme1` 全部保留）
2. 之後每份來源，**只搬 `slideN.xml` 與它 rels 指到的 media**，把 `rId1 → ../slideLayouts/slideLayout7.xml` 保留指向基底同一份
3. 重新編號 slide part（`slide2.xml`、`slide3.xml`…）、重新配置 `r:embed` 的 rId
4. 在 `p:sldIdLst` 追加 `<p:sldId r:id="rIdN" id="NNN"/>`，並在 `presentation.xml.rels` 補對應關係
5. `[Content_Types].xml` 為每個新 slide 加一條 `Override`

結果：11 layouts + 1 master + 1 theme。**檔案大小約為 Spire 版的 40%，且 PowerPoint 開啟更快。**

唯一的失敗條件是「來源不相容」——例如某份 PPTX 的 layout 集合不同。此時要 fallback 到完整 part 匯入。建議：合併前先比對 `slideLayout7.xml` 的雜湊，不一致就退回完整匯入。

### 合併後的必要清理

Spire 會在 `ppt/` 留下 `tags/tag1.xml`、`docProps/thumbnail.jpeg`，並在 `presentation.xml` 寫入指紋註解：

```xml
<!--Generated by Spire.Presentation for .NET 11.8.0.0-->
```

Rust 版不需要這些。

---

## 7. 形狀走訪與換字型（`refine_ppt`）

需要的操作：

| 操作 | XML |
| :--- | :--- |
| 走訪所有形狀 | `p:spTree` 的 `p:sp` / `p:pic` / `p:graphicFrame` / `p:grpSp` |
| 判斷是否為文字框 | 有 `<p:txBody>` 子節點（Spire 用 `"IAutoShape" in str(type(shape))`） |
| 刪除非文字框 | 從 `spTree` 移除節點 |
| 換字型 | 每個 `<a:rPr>`（或 `<a:defRPr>`）的 `<a:latin @typeface>` |
| 讀既有背景圖 | `p:bg/p:bgPr/a:blipFill/a:blip/@r:embed` → 對應 rels → `ppt/media/*` |

⚠️ Spire 的「形狀索引」是**只算 `p:sp`**還是**算全部節點**，會影響 `refine_ppt` 裡 `slide.Shapes[i]` 的語意。Rust 實作時建議改用「先用 `p:cNvPr/@name` 過濾出 `Block_*` 開頭者」取代索引，語意更穩定。這是刻意的偏離，需在 golden fixture 上驗證視覺相同。

---

## 8. `presentation.xml` 的最小骨架

Spire 產出的基準檔含 11 個 layout、1 個 master、`presProps`、`viewProps`、`tableStyles`、`tags`、`theme`。若要從零產生（而非從範本增修），最小可用集合是：

```
[Content_Types].xml
_rels/.rels
ppt/presentation.xml
ppt/_rels/presentation.xml.rels
ppt/slideMasters/slideMaster1.xml  (+ _rels)
ppt/slideLayouts/slideLayout1..N.xml (+ _rels)
ppt/theme/theme1.xml
ppt/slides/slideN.xml (+ _rels)
ppt/media/*                        (若有圖)
```

`presProps.xml`、`viewProps.xml`、`tableStyles.xml`、`docProps/*` 皆為**選用**，PowerPoint 缺了也能開，但部分工具（LibreOffice 匯出、縮圖預覽）行為會不同。建議 Rust 版保留 `presProps` 與 `docProps`，其餘可省。

---

## 附：完整輸出樣本

| 檔案 | 內容 | 對應操作 |
| :--- | :--- | :--- |
| `../fixtures/probe-pptx/1_text.pptx` | 單頁 + 文字框 | §2 |
| `../fixtures/probe-pptx/2_background.pptx` | 單頁 + 圖片背景 | §3 |
| `../fixtures/probe-pptx/3_picture.pptx` | 單頁 + 置底插圖 | §4 |
| `../fixtures/probe-pptx/4_merged.pptx` | 3 份合併（master 各自獨立） | §6 |
| `../fixtures/probe-pptx/5_cleaned.pptx` | 合併後刪水印 | §5 |
| `../fixtures/probe-pptx/sz_4x3.pptx` | 4:3 畫布 | §1 |
| `../fixtures/probe-pptx/sz_16x10.pptx` | 16:10 畫布 | §1 |
| `../fixtures/probe-pptx/sz_16x9.pptx` | 16:9 畫布 | §1 |

每份都可以 `unzip -o <file>.pptx -d out/` 後直接讀 XML。

---

## 9. 多行文字（`TextFrame.Text` 含 `\n`）

**實測：Spire 不拆段落。** 換行字元原樣寫進單一 `<a:t>`：

```xml
<a:p>
  <a:pPr algn="ctr"/>
  <a:r>
    <a:rPr lang="en-US" dirty="1"/>
    <a:t>line one
line two</a:t>
  </a:r>
</a:p>
```

樣本：`../fixtures/probe-pptx/6_newline.pptx`。

這是 PowerPoint 與 LibreOffice 都容忍、但 DrawingML 規格上不算正規的寫法 ——
換行的正式表示法是 `<a:br/>`。

`n2p-ooxml` 因此提供三種策略（`shape::NewlineMode`）：

| 模式 | 產出 | 用途 |
| :--- | :--- | :--- |
| `LineBreak`（預設） | 單段落 + `<a:br/>` | 規格正確，視覺與 Spire 相同 |
| `Literal` | 單段落 + 換行字元在 `a:t` 內 | 與 Spire 逐位元等價，回歸比對用 |
| `Paragraph` | 每個換行一段 `a:p` | 需要獨立段落間距時 |

⚠️ 這個差異會影響 `ppt_creater.py`：MinerU 的 `block_content` 經常帶 `\n`，
目前是走 Spire 的 `Literal` 行為。改成 `LineBreak` 前應在 golden fixture 上比對視覺。
