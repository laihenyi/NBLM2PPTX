//! 用 Microsoft PowerPoint（macOS）開檔並讀回內容。
//!
//! ## 為什麼不是「叫 PowerPoint 產生 PDF」
//!
//! PowerPoint 16.101 的 AppleScript 字典裡**沒有**文件層級的 `save`／`export`
//! 命令（只有 `save as picture` 與主題的存檔），因此無法腳本化匯出 PDF。
//! 但「開檔 → 讀回頁數／畫布尺寸／形狀名稱／文字」完全可行，而這正好回答
//! 本專案最需要確認的問題：**PowerPoint 是否接受我們產生的 OOXML**。
//!
//! 若需要像素級的視覺比對，請用 `visual-diff`（LibreOffice 後端）。
//!
//! ## 輸出協定
//!
//! 一次 osascript 呼叫處理一個檔案，回傳以 US（ASCII 31）分隔欄位、
//! RS（ASCII 30）分隔紀錄的字串，避免文字內容含分隔符。
//!
//! ## 安全性：只關閉自己開的文件
//!
//! 腳本會先記下「執行前已經開著的簡報」，之後只關閉自己開的那一份。
//! **絕對不做「關閉全部簡報」** —— 使用者的 PowerPoint 裡可能有未儲存的文件，
//! 那種迴圈會直接把別人的工作丟掉。同理，這裡也從不呼叫 `quit`。

//! ## macOS 的限制：拿不到畫布高度
//!
//! `page setup` 類別在這個版本只有 `slide width`，**沒有 `slide height`**
//! （Windows 的 VBOM 兩者都有）。存取 `slide height` 會得到
//! `The object you are trying to access does not exist`。
//! 因此這裡只回報寬度，高度改由其他方式確認（例如 `python-pptx` 或本專案自己的解析）。

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow};

const FIELD_SEP: char = '\u{1f}';
const RECORD_SEP: char = '\u{1e}';

/// 一個檔案的讀回結果。
#[derive(Debug, Clone, PartialEq)]
pub struct PresentationFacts {
    pub slides: usize,
    /// 畫布寬度（點）。高度無法透過 AppleScript 取得，見模組說明。
    pub width_pt: f64,
    /// `slide size` 列舉的文字形式（例如 `slide size custom`），僅供記錄。
    pub slide_size_kind: String,
    pub pages: Vec<SlideFacts>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlideFacts {
    /// 形狀名稱（依 z 序）。
    pub shape_names: Vec<String>,
    /// 文字形狀的內容，已做空白正規化。
    pub texts: Vec<String>,
}

/// PowerPoint 是否可用（回傳版本字串）。
pub fn version() -> Result<String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(r#"tell application "Microsoft PowerPoint" to get version"#)
        .output()
        .context("執行 osascript 失敗")?;
    if !output.status.success() {
        return Err(anyhow!(
            "無法取得 PowerPoint 版本：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

const SCRIPT: &str = r#"on run argv
	-- 變數必須在 tell 之前指派，否則會被解析成 PowerPoint 的屬性。
	-- 逐形狀 try：Spire 評估水印的 a:spLocks noTextEdit="1" 會讓索引存取拋 -1728。
	-- 成敗都要關閉自己開的那一份，否則會累積殭屍簡報與 ~$ 鎖檔。
	set thePath to item 1 of argv
	set slideCount to 0
	set frameWidth to 0
	set kindText to ""
	set pageNames to {}
	set pageTexts to {}
	set outText to ""
	set wasOpen to false
	set failureMessage to ""
	set fs to (ASCII character 31)
	set rs to (ASCII character 30)

	with timeout of 180 seconds
	tell application "Microsoft PowerPoint"
		set openCount to (count of presentations)
		repeat with k from 1 to openCount
			try
				if (full name of presentation k as text) is thePath then set wasOpen to true
			end try
		end repeat
		open thePath

		set ready to false
		set attempts to 0
		set pres to missing value
		repeat while ready is false and attempts < 100
			try
				set candidate to active presentation
				set candidateSlides to (count of slides of candidate)
				set candidateWidth to (slide width of page setup of candidate)
				if candidateWidth > 0 then
					set pres to candidate
					set slideCount to candidateSlides
					set frameWidth to candidateWidth
					set ready to true
				end if
			end try
			if ready is false then
				delay 0.2
				set attempts to attempts + 1
			end if
		end repeat

		if ready is false then
			set failureMessage to "簡報在 20 秒內未就緒"
		else
			try
				set kindText to ((slide size of page setup of pres) as text)
				repeat with i from 1 to slideCount
					set s to slide i of pres
					set nameBuf to {}
					set textBuf to {}
					set shapeCount to (count of shapes of s)
					repeat with j from 1 to shapeCount
						-- 逐形狀 try：被鎖定的形狀（例如 Spire 評估水印的
						-- a:spLocks noTextEdit="1"）會讓索引存取直接拋 -1728。
						try
							set sh to shape j of s
							set end of nameBuf to (name of sh)
							try
								if has text frame of sh is true then
									set tc to content of text range of text frame of sh
									if tc is not missing value then set end of textBuf to tc
								end if
							end try
						on error
							set end of nameBuf to "<inaccessible>"
						end try
					end repeat
					set end of pageNames to nameBuf
					set end of pageTexts to textBuf
				end repeat
			on error m
				set failureMessage to m
			end try
		end if

		-- 不論成敗都要關閉自己開的那一份，否則會累積殭屍簡報與 ~$ 鎖檔
		if wasOpen is false and pres is not missing value then
			try
				close pres saving no
			end try
		end if
	end tell
	end timeout

	if failureMessage is not "" then error failureMessage

	set outText to "HEAD" & fs & (slideCount as text) & fs & (frameWidth as text) & fs & kindText & rs
	repeat with i from 1 to (count of pageNames)
		set outText to outText & "SLIDE" & fs & (i as text) & fs & "NAMES" & fs
		repeat with nm in (item i of pageNames)
			set outText to outText & (nm as text) & fs
		end repeat
		set outText to outText & "TEXTS" & fs
		repeat with tx in (item i of pageTexts)
			set outText to outText & (tx as text) & fs
		end repeat
		set outText to outText & rs
	end repeat
	return outText
end run
"#;

/// 開一個 pptx 並讀回頁數、畫布尺寸、形狀名稱、文字。
///
/// 路徑會被正規化成絕對路徑再交給 AppleScript。**這不是可選的**：
/// AppleScript 的 `open` 對相對路徑的處理是把 `/` 接在前面（`../x` → `/../x`），
/// PowerPoint 讀不到就彈出 modal 錯誤對話框，而那個對話框會讓之後**所有**
/// AppleScript 呼叫都失敗（`-1728`、`-1712`、`-9074`）。
pub fn inspect(path: &Path) -> Result<PresentationFacts> {
    if !path.exists() {
        return Err(anyhow!("檔案不存在：{}", path.display()));
    }
    let path = path
        .canonicalize()
        .with_context(|| format!("無法正規化路徑：{}", path.display()))?;
    let output = Command::new("osascript")
        .arg("-e")
        .arg(SCRIPT)
        .arg(&path)
        .output()
        .context("執行 osascript 失敗")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let hint =
            if stderr.contains("-9074") || stderr.contains("逾時") || stderr.contains("-1712") {
                "（PowerPoint 可能卡在對話框：請手動結束 PowerPoint 後重試）"
            } else {
                ""
            };
        return Err(anyhow!(
            "PowerPoint 開檔失敗（{}）：{stderr}{hint}",
            path.display()
        ));
    }

    let raw = String::from_utf8_lossy(&output.stdout).into_owned();
    parse_output(&raw).with_context(|| format!("解析 PowerPoint 輸出失敗：{}", path.display()))
}

/// 解析 AppleScript 的回傳字串。抽成獨立函式以便單元測試。
pub fn parse_output(raw: &str) -> Result<PresentationFacts> {
    let mut slides = None;
    let mut width_pt = None;
    let mut slide_size_kind = None;
    let mut pages = Vec::new();

    for record in raw.split(RECORD_SEP) {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let mut fields = record.split(FIELD_SEP);
        match fields.next().map(str::trim) {
            Some("HEAD") => {
                slides = Some(
                    fields
                        .next()
                        .ok_or_else(|| anyhow!("HEAD 缺少頁數"))?
                        .trim()
                        .parse::<usize>()?,
                );
                width_pt = Some(
                    fields
                        .next()
                        .ok_or_else(|| anyhow!("HEAD 缺少寬度"))?
                        .trim()
                        .parse::<f64>()?,
                );
                slide_size_kind = Some(
                    fields
                        .next()
                        .ok_or_else(|| anyhow!("HEAD 缺少 slide size"))?
                        .trim()
                        .to_owned(),
                );
            }
            Some("SLIDE") => {
                let _index: usize = fields
                    .next()
                    .ok_or_else(|| anyhow!("SLIDE 缺少索引"))?
                    .trim()
                    .parse()?;
                // 之後的欄位是 `NAMES <fs> name... <fs> TEXTS <fs> text...`；
                // 名稱／文字可能為空，因此用明確標記而不是「空欄位」分隔。
                let mut shape_names = Vec::new();
                let mut texts = Vec::new();
                let mut in_texts = false;
                for field in fields {
                    let value = field.trim();
                    match value {
                        "NAMES" => {
                            in_texts = false;
                            continue;
                        }
                        "TEXTS" => {
                            in_texts = true;
                            continue;
                        }
                        _ => {}
                    }
                    if value.is_empty() {
                        continue;
                    }
                    if in_texts {
                        texts.push(normalize_whitespace(value));
                    } else {
                        shape_names.push(value.to_owned());
                    }
                }
                pages.push(SlideFacts { shape_names, texts });
            }
            _ => {}
        }
    }

    Ok(PresentationFacts {
        slides: slides.ok_or_else(|| anyhow!("輸出缺少 HEAD 紀錄"))?,
        width_pt: width_pt.unwrap_or(0.0),
        slide_size_kind: slide_size_kind.unwrap_or_default(),
        pages,
    })
}

/// 把連續空白（含 PowerPoint 的 `\r` 段落分隔）壓成單一空格。
///
/// PowerPoint 的 `content` 用 `\r` 分段，AppleScript 回傳時可能被轉成換行，
/// 因此比對前統一把空白正規化，只比較「文字內容」本身。
pub fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_slides() {
        let fs = FIELD_SEP;
        let rs = RECORD_SEP;
        let raw = format!(
            "HEAD{fs}2{fs}960.0{fs}slide size custom{rs}SLIDE{fs}1{fs}NAMES{fs}Block_text{fs}TEXTS{fs}穿透表象 標題{fs}{rs}SLIDE{fs}2{fs}NAMES{fs}Block_title{fs}Block_text{fs}TEXTS{fs}第一段 第二段{fs}{rs}SLIDE{fs}3{fs}NAMES{fs}{fs}TEXTS{fs}{fs}{rs}"
        );
        let facts = parse_output(&raw).unwrap();
        assert_eq!(facts.slides, 2);
        assert_eq!(facts.width_pt, 960.0);
        assert_eq!(facts.slide_size_kind, "slide size custom");
        assert_eq!(facts.pages.len(), 3);
        assert_eq!(facts.pages[0].shape_names, vec!["Block_text".to_string()]);
        assert_eq!(facts.pages[0].texts, vec!["穿透表象 標題".to_string()]);
        assert_eq!(
            facts.pages[1].shape_names,
            vec!["Block_title", "Block_text"]
        );
        assert_eq!(facts.pages[1].texts, vec!["第一段 第二段".to_string()]);
        // 空頁：沒有任何形狀與文字
        assert!(facts.pages[2].shape_names.is_empty());
        assert!(facts.pages[2].texts.is_empty());
    }

    #[test]
    fn missing_header_is_rejected() {
        assert!(parse_output("garbage").is_err());
        assert!(parse_output("").is_err());
    }

    #[test]
    fn whitespace_normalisation_collapses_runs() {
        assert_eq!(normalize_whitespace("a\r\rb\n\n c \t d"), "a b c d");
        assert_eq!(normalize_whitespace("   "), "");
    }
}
