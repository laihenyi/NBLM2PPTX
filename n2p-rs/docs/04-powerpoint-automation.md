# 與 Microsoft PowerPoint 的關係

## 為什麼不用 PowerPoint 產生 PPTX

舊版（Windows）流程裡，PowerPoint 從來不是「產生可編輯文字」的那一環。三處跟它有關的程式碼：

| 位置 | 做什麼 | 性質 |
| :--- | :--- | :--- |
| `screenshot_automation.py:62` `get_ppt_windows()` | `EnumWindows` 找 PowerPoint／WPS 視窗 | 事後問檔名 |
| `screenshot_automation.py:81` `get_all_open_ppt_info()` | COM 列舉 `Presentations` 取 `FullName` | 事後問檔名 |
| `screenshot_automation.py:213` `check_new_ppt_window()` | 輪詢等新視窗出現，當「完成」訊號 | 等待外部副作用 |
| `tests/compare_result.py:23` | COM 開檔 → `SaveAs(pdf, 32)` | 測試用渲染器 |

真正產生 `.pptx` 的是**微軟電腦管家的智慧圈選**（`Ctrl+Shift+A` → 轉換為 PPT），PowerPoint 只是接手開啟。而**PowerPoint 不會 OCR** —— 本專案的核心問題是「像素文字 → 可編輯文字框」，那不是它的能力範圍。

macOS 版因此把兩件事都自己做：文字用 Vision OCR，檔案用 `n2p-ooxml` 寫。理由：

1. **需求面很小。** 全部只有 8 個 OOXML 操作（見 `02-ooxml-reference.md`），`n2p-ooxml` 不到 3,000 行就做完，且與 Spire 輸出逐節點相同。
2. **可測性。** 自建寫入器的 golden test 可以逐位元斷言；插一個 PowerPoint 進去就變成「與 PowerPoint 版本 N 的輸出比對」，無法在 CI 重現。
3. **不必重現舊版的脆弱度。** 舊版用 70 行輪詢只為了回答「它存了哪個檔」，還需要「沒偵測到視窗就用 `pywinauto` 點關閉按鈕」的補救。自建寫入器沒有這些問題 —— 檔名是自己寫的。
4. **部署。** 舊版把「Microsoft PowerPoint 或 WPS Office」列為系統需求；macOS 版只需要 macOS。CI runner 上也裝不了 Office。
5. **速度。** 整份 2 頁 PDF（點陣化＋去水印＋Vision OCR＋產生 PPTX）2.1 秒。

## 但有兩件事仍然「借用」PowerPoint 的能力

* **autofit。** 文字自動縮放是專有演算法。我們只寫 `<a:normAutofit/>` 這個語意，實際縮多少由開啟檔案的 PowerPoint 計算（`--autofit`）。
* **既有模板就地增修。** `Deck::open()` 讀使用者真正的 `.pptx`，只改要改的節點，master／theme／字型原封不動。不需要「用 PowerPoint 重建一份一樣的」。

## 唯一該用 Office 的地方：驗證

`xtask` 提供兩條互補的路徑：

| 子命令 | 後端 | 回答的問題 |
| :--- | :--- | :--- |
| `compat` | Microsoft PowerPoint（AppleScript） | PowerPoint 接受我們產生的 OOXML 嗎？讀回來的結構對嗎？ |
| `visual-diff` | LibreOffice（`soffice`） | 渲染出來看起來一樣嗎？ |

兩者不能合併，因為 macOS 的 PowerPoint **無法腳本化匯出 PDF**（見下）。

## macOS PowerPoint AppleScript 的四個坑

實測於 PowerPoint 16.101。每一項都花了時間才找出來，記在這裡避免重踩。

### 1. `page setup` 沒有 `slide height`

```applescript
slide width  of page setup of pres   -- ✓ 回 960.0
slide height of page setup of pres   -- ✗ The object you are trying to access does not exist
```

Windows 的 VBOM 兩者都有，macOS 的字典只有寬度。因此相容性檢查只驗寬度，高度改用其他方式確認。

### 2. 沒有文件層級的 `save` / `export`

`sdef` 裡只有 `save as picture`（形狀）與主題的色彩／字型存檔。`save as PDF` 這個列舉值屬於 `MsoPictureType`，是給 `save as picture` 用的。

結論：**無法用 AppleScript 把 pptx 匯出成 PDF**。像素比對只能靠 LibreOffice。

### 3. `repeat with x in (collection)` 會卡死

```applescript
repeat with sh in (shapes of slide 1 of pres)   -- ✗ 實測 3 分鐘未返回
```

改成索引存取後 0.88 秒完成：

```applescript
set shapeCount to (count of shapes of s)
repeat with j from 1 to shapeCount
    set sh to shape j of s
    ...
end repeat
```

同樣規則適用於 `presentations` 集合。

### 4. `tell` 區塊裡的變數作用域

```applescript
on run argv
    set fs to (ASCII character 31)     -- 必須在 tell 之前指派
    tell application "Microsoft PowerPoint"
        set outText to "HEAD" & fs     -- 否則 fs 會被解析成 PowerPoint 的屬性 → missing value
    end tell
end run
```

所有變數都要在 `tell` 區塊之前先指派；`set my x` 在 `on run` 裡無效（腳本物件沒有這個屬性，會回 `cannot set`）。

### 5. `open` 是非同步的

固定 `delay 0.4` 不夠。單頁檔案在載入完成前：

```applescript
count of slides of pres          -- 回 0
slide width of page setup of pres -- 拋 -1728
```

必須輪詢到「頁數與畫布寬度都讀得到」為止：

```applescript
repeat while ready is false and attempts < 100
    try
        set candidate to active presentation
        set candidateSlides to (count of slides of candidate)
        set candidateWidth to (slide width of page setup of candidate)
        if candidateWidth > 0 then set ready to true
    end try
    if ready is false then
        delay 0.2
        set attempts to attempts + 1
    end if
end repeat
```

### 6. 相對路徑會讓 PowerPoint 彈出 modal，然後全部卡死

```applescript
open "../rust-rewrite/assets/probe-pptx/1_text.pptx"
```

AppleScript 會把它解析成 `/../rust-rewrite/...`（把 `/` 接在前面），PowerPoint 讀不到就跳出
「很抱歉，PowerPoint 無法讀取…」的 modal。**那個 modal 一掛上去，之後每一個 AppleScript
呼叫都會失敗**（`-1728`、`-1712`、`-9074`），而且必須人工點掉。

因此所有路徑都必須先 `canonicalize()` 成絕對路徑。這一條也解釋了為什麼「PowerPoint 好像
卡住了」其實不是 PowerPoint 的問題。

### 7. 被鎖定的形狀會讓索引存取拋 `-1728`

Spire 評估水印的形狀帶 `a:spLocks noSelect="1" noMove="1" noResize="1" noTextEdit="1"`，
`shape j of s` 取到它就直接失敗。逐形狀 `try` 是必要的，並記成 `<inaccessible>` 保留形狀數量。

## 安全規則：只關閉自己開的文件

第一版腳本用了「關閉全部簡報」的迴圈來清掉上一次逾時留下的殘留：

```applescript
repeat while (count of presentations) > 0
    close presentation 1 saving no    -- ✗ 危險
end repeat
```

**這會丟掉使用者未儲存的工作。** 現在的做法：

1. 執行前先記下已開著的簡報（用 `full name` 比對）。
2. 只關閉「本來沒開、由這次 `open` 帶進來的」那一份。
3. 永不呼叫 `quit`。

驗證工具沒有資格碰使用者的文件。

還有一條同樣重要：**成敗都要關閉**。AppleScript 沒有 `finally`，第一版在讀取失敗時直接
中止，`close` 那行根本沒跑到 —— 結果累積了 12 份殭屍簡報與 12 個 `~$` 鎖檔，而鎖檔又讓
下一次 `open` 失敗，形成連鎖。現在的結構是：

```applescript
try
    -- 收集資料
on error m
    set failureMessage to m
end try
if wasOpen is false and pres is not missing then
    try
        close pres saving no
    end try
end if
```

## 目前狀態

三項驗證全部通過（PowerPoint 16.101，macOS arm64）：

```
$ cargo run -p xtask -- compat
[image]    ✓ PowerPoint 開啟成功（寬 960 pt、0 形狀、0 文字框）
[vision]   ✓ PowerPoint 開啟成功（寬 960 pt、1 形狀、1 文字框）
全部通過

$ cargo run -p xtask -- compat --record --dir ../rust-rewrite/assets/probe-pptx
已寫入 17 筆

$ cargo run -p xtask -- compat --dir ../rust-rewrite/assets/probe-pptx
17 項全部 ✓
```

執行後 PowerPoint 沒有殘留開啟的簡報，也沒有 `~$` 鎖檔。

### 視覺回歸（LibreOffice 26.8.1.1）

```
$ cargo run -p xtask -- visual-diff --dir ../rust-rewrite/assets/probe-pptx --record
已記錄 17 個檔案、25 頁

$ cargo run -p xtask -- visual-diff --dir ../rust-rewrite/assets/probe-pptx
sz_4x3   第 1 頁 ✓ 平均差 0.00、顯著差異 0.00%
…
全部通過
```

**25 頁的平均差全部是 0.00** —— LibreOffice 的渲染逐位元可重現，所以這個 harness 沒有雜訊，
任何非零差異都代表真的改到了東西。基準圖 400 KB。

基準圖旁邊會自動寫一份 `meta.json`，記下產生它的 LibreOffice 版本與 DPI。比對時若環境
不符會先警告，避免「換版本 → 一堆無法解釋的失敗」：

```
$ cargo run -p xtask -- visual-diff --file 1_text.pptx --dpi 150
  ⚠ 參考圖以 100 dpi 產生，目前是 150 dpi，比對結果不可信
  第 1 頁 ✗ 尺寸不符
```

CI 上另有一個 `visual-regression` job，只在手動觸發（`workflow_dispatch`）時跑 ——
LibreOffice 的安裝要 1–3 分鐘，不值得拖慢每次 push。

### 順帶修掉的 fixture 問題

`sz_4x3.pptx`／`sz_16x10.pptx`／`sz_16x9.pptx` 原本是 **0 頁**的檔案 —— 產生它們的腳本
執行 `while p.Slides.Count: RemoveAt(0)` 之後忘了 `Append()`，只設定了畫布尺寸就存檔。
PowerPoint 回報 0 頁是正確的。已重新產生為各 1 頁。
