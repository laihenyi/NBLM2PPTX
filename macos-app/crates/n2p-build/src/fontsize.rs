//! 字級估算。
//!
//! ## 為什麼不能只看邊界框高度
//!
//! Python 版（以及本專案的第一版）用 `font_size = bbox高度 × 0.75`。這在
//! MinerU 那種「區塊框含行距」的座標上勉強可用，但換成 Vision 的「貼齊墨跡」框就
//! 系統性偏小 —— 對 CJK 等於先假設墨跡只有 0.75 em，又再乘一次 0.75。
//!
//! 更根本的問題是**墨跡高度本身不可靠**：同一字級下，`一二三` 的墨跡約 0.6 em，
//! `提學國` 接近 1.0 em。用高度推字級必然有 ±20% 的雜訊。
//!
//! ## 改用字寬
//!
//! CJK 是等寬全形，**一個字的寬度就是一個 em**。所以
//!
//! ```text
//! em ≈ 行寬 / 全形字數（半形字元折算）
//! ```
//!
//! 這個估法對中文為主的簡報幾乎是精確的，而且與字型、粗細無關。
//! 實測（`CH9_用 2.0 版的愛打造正向課堂.pdf` 第 1 頁）：
//!
//! | 文字 | 全形單位 | 框寬 pt | 推得 em |
//! | :--- | ---: | ---: | ---: |
//! | 提升學生正向幸福 | 8.0 | 335.7 | 42.0 |
//! | 感的課堂領導力 | 7.0 | 298.6 | 42.7 |
//!
//! 兩行本該同級，推得 42.0 與 42.7，誤差 1.7%。
//!
//! 半形字元的折算寬度採近似值：數字與大寫 0.55、`i l . , '` 等窄字 0.28、
//! `m w M W` 0.85、其餘 0.52。對「中文夾一點英文數字」的簡報夠用。

/// 半形字元的近似前進寬度（單位：em）。
fn half_width_advance(c: char) -> f64 {
    match c {
        ' ' => 0.28,
        'i' | 'l' | 'j' | 'I' | '!' | '.' | ',' | ':' | ';' | '\'' | '"' | '|' | '/' | '\\' => 0.28,
        'm' | 'w' | 'M' | 'W' => 0.85,
        'f' | 'r' | 't' | '(' | ')' | '[' | ']' | '-' | '–' => 0.35,
        c if c.is_ascii_digit() => 0.55,
        c if c.is_ascii_uppercase() => 0.62,
        _ => 0.52,
    }
}

/// 是否為全形字元（CJK 漢字、全形標點、假名、諺文）。
pub fn is_full_width(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F        // 諺文字母
        | 0x2E80..=0x303E      // CJK 部首、標點（含全形逗號句號）
        | 0x3041..=0x33FF      // 假名、CJK 相容
        | 0x3400..=0x4DBF      // CJK 擴充 A
        | 0x4E00..=0x9FFF      // CJK 基本
        | 0xA000..=0xA4CF      // 彝文
        | 0xAC00..=0xD7A3      // 諺文音節
        | 0xF900..=0xFAFF      // CJK 相容表意
        | 0xFE30..=0xFE4F      // 全形標記
        | 0xFF00..=0xFF60      // 全形 ASCII
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x3FFFD    // CJK 擴充 B 以後
    )
}

/// 一行文字的「全形單位」數。
pub fn full_width_units(text: &str) -> f64 {
    text.chars()
        .filter(|c| !c.is_control())
        .map(|c| {
            if is_full_width(c) {
                1.0
            } else {
                half_width_advance(c)
            }
        })
        .sum()
}

/// 全形字元佔比（0.0 ~ 1.0），用來決定該用哪種估法。
pub fn full_width_ratio(text: &str) -> f64 {
    let mut full = 0usize;
    let mut total = 0usize;
    for c in text
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
    {
        total += 1;
        if is_full_width(c) {
            full += 1;
        }
    }
    if total == 0 {
        0.0
    } else {
        full as f64 / total as f64
    }
}

/// 一行文字的估算結果，附上信心值（0.0 ~ 1.0）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub em_pt: f64,
    /// 估法的可靠度：字寬法高、墨跡高度法低。
    pub confidence: f64,
}

/// 由「行寬 + 內容」估字級。
///
/// 回傳 `None` 表示無法估算（空字串或寬度為 0）。
pub fn from_width(text: &str, width_pt: f64) -> Option<Estimate> {
    let units = full_width_units(text);
    if units < 0.5 || width_pt <= 0.0 || !width_pt.is_finite() {
        return None;
    }
    let em = width_pt / units;
    // 中文佔比越高，字寬法越可信（全形寬度固定）；純英文誤差較大。
    let ratio = full_width_ratio(text);
    let confidence = 0.55 + 0.45 * ratio;
    Some(Estimate {
        em_pt: em,
        confidence,
    })
}

/// 由墨跡高度估字級（備援）。
///
/// `ink_ratio` 是「墨跡高度 / em」的經驗值。CJK 行取 0.92（涵蓋大部分常用字），
/// 純英文行取 0.72（有升部與降部時的常見比例）。
pub fn from_ink_height(text: &str, ink_height_pt: f64, ink_ratio: f64) -> Option<Estimate> {
    if ink_height_pt <= 0.0 || ink_ratio <= 0.0 || !ink_height_pt.is_finite() {
        return None;
    }
    // 純英文行的墨跡高度變異極大（有沒有降部差很多），信心刻意壓低。
    let ratio = full_width_ratio(text);
    Some(Estimate {
        em_pt: ink_height_pt / ink_ratio,
        confidence: 0.25 + 0.35 * ratio,
    })
}

/// 給定一行文字的墨跡高度，回推適合的 `ink_ratio`。
pub fn ink_ratio_for(text: &str) -> f64 {
    let ratio = full_width_ratio(text);
    // 0.72（純拉丁）→ 0.92（純 CJK）
    0.72 + 0.20 * ratio
}

/// 多行時用行距（baseline 間距）估字級 —— 最穩，因為與字形無關。
///
/// PowerPoint 的單行行距約為 1.2 em（依字型的 ascent+descent 略有差異）。
pub fn from_line_pitch(pitch_pt: f64, pitch_ratio: f64) -> Option<Estimate> {
    if pitch_pt <= 0.0 || pitch_ratio <= 0.0 || !pitch_pt.is_finite() {
        return None;
    }
    Some(Estimate {
        em_pt: pitch_pt / pitch_ratio,
        confidence: 0.9,
    })
}

/// 合併兩個估算值：加權平均，權重取信心值。
pub fn blend(a: Option<Estimate>, b: Option<Estimate>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let wa = a.confidence.max(0.01);
            let wb = b.confidence.max(0.01);
            Some((a.em_pt * wa + b.em_pt * wb) / (wa + wb))
        }
        (Some(a), None) => Some(a.em_pt),
        (None, Some(b)) => Some(b.em_pt),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_width_detection() {
        assert!(is_full_width('提'));
        assert!(is_full_width('，'));
        assert!(is_full_width('！'));
        assert!(is_full_width('あ'));
        assert!(!is_full_width('A'));
        assert!(!is_full_width(','));
        assert!(!is_full_width('2'));
    }

    #[test]
    fn units_count_mixed_text() {
        // 8 個全形字
        assert_eq!(full_width_units("提升學生正向幸福"), 8.0);
        // 5 全形 + "2.0"（數字 0.55 + 句點 0.28 + 數字 0.55）
        let units = full_width_units("用2.0版的愛，");
        assert!(
            (units - (5.0 + 0.55 + 0.28 + 0.55)).abs() < 1e-9,
            "實際 {units}"
        );
    }

    #[test]
    fn width_estimate_matches_measured_values() {
        // 實測：框寬 335.7 pt、8 個全形字 → 應推得 41.96
        let estimate = from_width("提升學生正向幸福", 335.7).unwrap();
        assert!(
            (estimate.em_pt - 41.96).abs() < 0.01,
            "實際 {}",
            estimate.em_pt
        );
        assert!(estimate.confidence > 0.95, "純中文應高信心");

        // 實測：框寬 298.6 pt、7 個全形字 → 42.66
        let estimate = from_width("感的課堂領導力", 298.6).unwrap();
        assert!(
            (estimate.em_pt - 42.66).abs() < 0.01,
            "實際 {}",
            estimate.em_pt
        );
    }

    #[test]
    fn chinese_and_english_confidence_differ() {
        let zh = from_width("提升學生正向幸福", 336.0).unwrap();
        let en = from_width("Hello world", 336.0).unwrap();
        assert!(zh.confidence > en.confidence);
    }

    #[test]
    fn degenerate_inputs_return_none() {
        assert!(from_width("", 100.0).is_none());
        assert!(from_width("abc", 0.0).is_none());
        assert!(from_ink_height("abc", 0.0, 0.9).is_none());
        assert!(from_line_pitch(0.0, 1.2).is_none());
    }

    #[test]
    fn ink_ratio_interpolates_by_script() {
        let zh = ink_ratio_for("提升學生正向幸福");
        let en = ink_ratio_for("Hello world");
        assert!(zh > en, "中文墨跡比應高於英文：{zh} vs {en}");
        assert!((zh - 0.92).abs() < 0.05);
        assert!((en - 0.72).abs() < 0.05);
    }

    #[test]
    fn blend_prefers_higher_confidence() {
        let strong = Estimate {
            em_pt: 40.0,
            confidence: 0.9,
        };
        let weak = Estimate {
            em_pt: 20.0,
            confidence: 0.1,
        };
        let value = blend(Some(strong), Some(weak)).unwrap();
        // (40*0.9 + 20*0.1) / 1.0 = 38
        assert!((value - 38.0).abs() < 1e-9, "實際 {value}");
    }

    #[test]
    fn ink_height_alone_is_low_confidence() {
        let estimate = from_ink_height("提升學生正向幸福", 41.0, 0.92).unwrap();
        assert!(
            estimate.confidence < 0.7,
            "墨跡高度不該被高度信任：{}",
            estimate.confidence
        );
        assert!(
            estimate.em_pt > 40.0 && estimate.em_pt < 50.0,
            "實際 {}",
            estimate.em_pt
        );
    }

    #[test]
    fn blend_handles_missing_side() {
        let only = from_width("提升學生正向幸福", 336.0).unwrap();
        assert_eq!(blend(Some(only), None).unwrap(), only.em_pt);
        assert_eq!(blend(None, Some(only)).unwrap(), only.em_pt);
        assert!(blend(None, None).is_none());
    }
}
