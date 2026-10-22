# Spire.Presentation 11.8.1 引擎解析（macOS arm64 wheel）

> 目的：為 NotebookLM2PPT 的 Rust 改寫，把 `spire.presentation` 這個黑箱的**實際行為**取出成可實作的規格。
> 方法：REA（`../rea`）做二進位辨識 + 執行期行為取樣 + 輸出檔 OOXML 反推。
> 版本：`spire_presentation-11.8.1-py3-none-macosx_11_0_arm64.whl`（27.8 MB）

---

## 一、引擎架構：三層，不是你想的那種

下載 wheel 拆開後，只有兩個二進位檔與 435 個 Python 檔：

```
spire/presentation/lib/Spire.Presentation.Base.dylib   48.6 MB   Mach-O arm64
spire/presentation/lib/libSkiaSharp.dylib              15.2 MB   universal (x86_64 + arm64)
spire/presentation/**/*.py                              435 檔   ctypes 代理層
```

`file` 判定：

```
Spire.Presentation.Base.dylib: Mach-O 64-bit dynamically linked shared library arm64
libSkiaSharp.dylib:            Mach-O universal binary with 2 architectures
```

**關鍵判定：`.NET NativeAOT` 編譯出的原生動態庫，不是受控組件。**

這件事決定了可行的分析路線：

| 假設 | 實際 | 影響 |
| :--- | :--- | :--- |
| 是 .NET 受控 DLL，可反編譯成 C# | ❌ 是 NativeAOT 原生機器碼 | `inspect-managed-members`／ILSpy 全部無效 |
| 有 CIL metadata 可讀型別資訊 | ❌ 無 metadata | 沒有欄位表、沒有方法簽章可讀 |
| 需要反編譯機器碼才能理解 | ❌ 不需要 | 見下節：介面與行為都可完整取得 |

換句話說，**這個引擎沒有「原始碼」可解譯**，但它有兩樣東西比原始碼更有用：完整的 C ABI 匯出表，以及可觀測的輸出。

---

## 二、第一層：C ABI 匯出表（3,927 個符號）

NativeAOT 把每個 .NET 公開成員編譯成一個扁平 C 函式，命名規則固定：

```
<Type>_<Member>            例：Presentation_get_SlideSize
<Type>_<Member><後綴>      例：SlideList_AppendS   (S = Slide 參數)
                                ShapeList_AppendEmbedImageSFR  (S=Stream, F=File?, R=Rect)
```

抽法（無需任何逆向工具）：

```bash
nm -gU Spire.Presentation.Base.dylib | awk '{print $3}' | sed 's/^_//'
```

完整清單保存在 [`research/spire-11.8.1-exports.txt`](research/spire-11.8.1-exports.txt)（3,927 行）。

### 型別分佈（前 30）

| 型別 | 匯出數 | 型別 | 匯出數 |
| :--- | ---: | :--- | ---: |
| `Color` | 165 | `TextParagraphProperties` | 48 |
| `Presentation` | 109 | `Regex` | 47 |
| `IChartAxis` | 93 | `CultureInfo` | 45 |
| `ChartAxis` | 90 | `ChartDataLabel` | 43 |
| `IChart` | 86 | `IEmbedImage` | 43 |
| `DateTime` | 69 | `Rectangle` | 42 |
| `ChartSeriesDataFormat` | 64 | `RectangleF` | 42 |
| `IVideo` | 60 | `ITextFrameProperties` | 40 |
| `ITable` | 60 | `Cell` | 40 |
| `TextCharacterProperties` | 58 | `Encoding` | 38 |
| `Char` | 58 | `TextLineFormat` | 37 |
| `ShapeList` | 57 | `ClickHyperlink` | 34 |
| `Shape` / `IShape` | 52 | `ReflectionEffect` | 30 |
| `IAutoShape` | 49 | `Double` | 30 |

**結論：這個引擎有 60% 的表面在圖表（Chart）上，而本專案一個都沒用到。** 真正需要的只有 `Presentation`、`SlideList`、`ShapeList`、`IAutoShape`、`TextFrame`、`TextRange`、`SlideBackground`、`EmbedImageList`、`SlideSize`、`RectangleF` 這十個型別，約 400 個匯出。

### 本專案實際用到的匯出（逐一對上）

| repo 呼叫 | 實際 C 符號 | 參數 |
| :--- | :--- | :--- |
| `Presentation()` | `Presentation_CreatePresentation` | — |
| `LoadFromFile(path)` | `Presentation_LoadFromFile` | `(c_void_p, c_char_p)` |
| `SaveToFile(path, fmt)` | `Presentation_SaveToFileFF` | `(c_void_p, c_char_p, c_int)` |
| `Slides.RemoveAt(i)` | `SlideList_RemoveAt` | `(c_void_p, c_int)` |
| `Slides.Append()` | `SlideList_Append` | `(c_void_p)` |
| `Slides.AppendBySlide(s)` | `SlideList_AppendS` | `(c_void_p, c_void_p)` |
| `Shapes.AppendShape(type, rect)` | `ShapeList_AppendShape` | `(c_void_p, c_int, c_void_p)` |
| `Shapes.AppendEmbedImageByPath(...)` | `ShapeList_AppendEmbedImageSFR` | `(c_void_p, c_char_p, c_void_p…)` |
| `Shapes.RemoveAt(i)` | `ShapeList_RemoveAt` | `(c_void_p, c_int)` |
| `Images.AppendStream(stream)` | `EmbedImageList_Append` | `(c_void_p, c_void_p)` |
| `ZOrderPosition = 0` | `IAutoShape_set_ZOrderPosition` | `(c_void_p, c_int)` |
| `SlideBackground.Type` | `SlideBackground_set_Type` | `(c_void_p, c_int)` |
| `SlideSize.Type` | `SlideSize_set_Type` | `(c_void_p, c_int)` |

完整對照可由 `spire/presentation/SlideList.py` 等 435 個 Python 檔逐行讀出——**這些檔案就是這個 ABI 的規格書，且是純文字可讀的**。

### 執行期驗證

本機（macOS arm64）可直接載入執行：

```bash
python3 -m venv /tmp/spire_venv
/tmp/spire_venv/bin/pip install spire_presentation-11.8.1-py3-none-macosx_11_0_arm64.whl
/tmp/spire_venv/bin/python -c "import spire.presentation as sp; p=sp.Presentation(); print(p.Slides.Count, p.SlideSize.Size.Width)"
# → 1 720.0
```

無需 Windows、無需 PowerPoint、無需 .NET runtime。

---

## 三、第二層：Python 綁定層（可完整閱讀）

`spire/presentation/Presentation.py` 這類檔案不是 stub，而是完整的呼叫約定，每個方法都寫明 ctypes 的 `argtypes` / `restype`：

```python
def SetPageSize(self, w: float, h: float, IsRatio: bool):
    GetDllLibPpt().Presentation_SetPageSize.argtypes = [c_void_p, c_float, c_float, c_bool]
    CallCFunction(GetDllLibPpt().Presentation_SetPageSize, self.Ptr, w, h, IsRatio)
```

所以整個引擎的**參數型別、順序、可選多載**都可以從這 435 個檔案直接讀出來，不需要任何反編譯。這比反編譯機器碼可靠得多。

多載後綴規則（從原始碼歸納）：

| 後綴 | 意義 | 例 |
| :--- | :--- | :--- |
| `S` | Slide 或 Stream 參數 | `SlideList_AppendS` |
| `F` | 檔案路徑（string） | `Presentation_LoadFromFileFF` |
| `P` | 密碼 | `Presentation_LoadFromFileFP` |
| `R` | Rectangle 參數 | `ShapeList_AppendEmbedImageSFR` |
| `A` | Audio | `WavAudioCollection_AppendA` |
| `T` | 範本／Type 列舉 | `SlideList_AppendT` |
| `L` | Layout | `SlideList_AppendSL` |
| `M` | Master | `SlideList_AppendSM` |
| 數字 | 版本化變體 | `ChartSeriesFormatCollection_AppendV11` |

---

## 四、第三層：輸出 OOXML（這一層才是 Rust 改寫真正要的）

前兩層只描述「怎麼呼叫」，第三層才描述「寫出什麼」。做法是餵輸入、取輸出、拆 zip、讀 XML。

複現腳本見 [`research/probe.py`](research/probe.py) 與 [`research/probe2.py`](research/probe2.py)，產出見 [`../fixtures/probe-pptx/`](../fixtures/probe-pptx/)。

結果整理在 [`02-ooxml-reference.md`](02-ooxml-reference.md)，發現見 [`03-findings-and-rust-plan.md`](03-findings-and-rust-plan.md)。

---

## 五、REA 的使用紀錄

```bash
# 1) Mach-O 結構與證據
node ../rea/scripts/rea.mjs inspect-macho \
  "/tmp/spire_dl/x/spire/presentation/lib/Spire.Presentation.Base.dylib"

# 2) 匯出符號（REA 的 managed 分析不適用，因此改用 nm）
nm -gU Spire.Presentation.Base.dylib | awk '{print $3}' | sed 's/^_//' > exports.txt
```

REA 的 `inspect-managed-*` / `project-managed-application-graph` 對這個 artifact **不可用**：`Spire.Presentation.Base.dylib` 是 NativeAOT 產物，沒有 PE/CLI metadata。REA 的 native 路徑（Hopper / Ghidra / IDA）在本機未安裝；且即使安裝，對 48 MB ARM64 機器碼做反編譯對本專案的邊際價值極低——因為輸出 XML 已經是完整規格。

---

## 六、授權與法律界線（必須先講清楚）

1. **不要**把反編譯出的機器碼邏輯抄進 Rust 實作。那會構成衍生作品，違反 e-iceblue 的商業授權。
2. **可以**做的事：觀測公開介面（匯出符號）、閱讀 wheel 內的 Python 綁定、用試用版產生檔案並分析**你擁有的輸出檔**。OOXML 是 ISO/IEC 29500 公開標準，實作該標準不侵犯 Spire 的著作權。
3. 這正是 Rust 改寫最大的實質收益：**用公開標準取代一個需要商業授權的黑箱**。
