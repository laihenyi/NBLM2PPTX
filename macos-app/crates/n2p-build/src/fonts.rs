//! 字型可用性偵測。
//!
//! ## 為什麼需要
//!
//! 把 `<a:ea>` 寫成一個沒安裝的字型，PowerPoint 會**靜默**落到主題字型 ——
//! 不會報錯，只是渲染結果與預期不同。實測本機沒有 `Microsoft JhengHei`，
//! 所以寫死這個名字等於沒設定。
//!
//! ## 為什麼是 Noto Sans TC
//!
//! NotebookLM（現在的 Gemini Notebook）產出的簡報是**圖片**，字型被畫進像素裡，
//! 沒有可讀的字型屬性。但它的文字外觀與 Google 自家字型一致，社群在處理繁中
//! 簡報時的共識也是 Noto Sans TC／思源黑體。且 Noto Sans CJK TC 同時包含完整
//! 拉丁字形，整份簡報可以只用這一個字型。
//!
//! ## 偵測方式
//!
//! macOS 的 `system_profiler SPFontsDataType` 要 8 秒，不能每次轉換都跑。
//! 這裡改掃標準字型目錄的檔名 —— 對這幾個候選字型而言檔名是穩定的，
//! 成本是一次 `read_dir`。

use std::path::{Path, PathBuf};

/// 東亞文字的語系偏好。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CjkScript {
    /// 繁體中文（預設）。
    TraditionalChinese,
    /// 簡體中文。
    SimplifiedChinese,
    /// 日文（含平假名、片假名）。
    Japanese,
}

/// 繁體中文候選字型，依偏好排序：(家族名稱, 檔名比對用的關鍵字)。
///
/// 偏好順序的理由：Noto Sans TC 最接近 NotebookLM 的實際外觀；其次是同樣設計的
/// 思源黑體；再來才是各系統的預設黑體。
pub const CANDIDATES: &[(&str, &[&str])] = &[
    (
        "Noto Sans TC",
        &[
            "notosanscjktc",
            "notosanstc",
            "notosanscjk-tc",
            "notosanshantc",
        ],
    ),
    (
        "Source Han Sans TC",
        &["sourcehansanstc", "sourcehansanstr", "sourcehansans-tw"],
    ),
    ("PingFang TC", &["pingfang"]),
    ("Microsoft JhengHei", &["jhenghei"]),
    ("Heiti TC", &["stheiti", "heiti"]),
    ("LiHei Pro", &["lihei"]),
];

/// 日文候選字型，依偏好排序。
///
/// macOS 的 APFS 檔名可能為 NFD（`ヒラギノ`），因此同時列出 NFC 與 NFD 關鍵字。
pub const CANDIDATES_JP: &[(&str, &[&str])] = &[
    (
        "Noto Sans JP",
        &[
            "notosanscjkjp",
            "notosansjp",
            "notosanscjk-jp",
            "notosanshanjp",
        ],
    ),
    (
        "Source Han Sans JP",
        &["sourcehansansjp", "sourcehansans-jp"],
    ),
    (
        "Hiragino Sans",
        &["hiraginosansw", "ヒラギノ角ゴシック", "ヒラギノ角ゴシック"],
    ),
    ("YuGothic", &["yugothic", "游ゴシック", "游ゴシック"]),
    ("Meiryo", &["meiryo", "メイリオ"]),
];

/// 簡體中文候選字型，依偏好排序。
pub const CANDIDATES_SC: &[(&str, &[&str])] = &[
    (
        "Noto Sans SC",
        &[
            "notosanscjksc",
            "notosanssc",
            "notosanscjk-sc",
            "notosanshansc",
        ],
    ),
    (
        "Source Han Sans SC",
        &["sourcehansanssc", "sourcehansanscn", "sourcehansans-cn"],
    ),
    ("PingFang SC", &["pingfang"]),
    ("Microsoft YaHei", &["yahei", "msyh"]),
    ("Heiti SC", &["stheiti", "heiti"]),
];

/// 文字中是否包含日文平假名或片假名。
pub fn contains_japanese_kana(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c as u32,
            0x3041..=0x3096 // 平假名
            | 0x30A1..=0x30FA // 片假名
            | 0x31F0..=0x31FF // 片假名拼音擴充
            | 0xFF66..=0xFF9F // 半形片假名
        )
    })
}

/// 由簡報內文判斷東亞文字語系：出現平假名／片假名即視為日文，否則為繁中預設。
pub fn detect_script(text: &str) -> CjkScript {
    if contains_japanese_kana(text) {
        CjkScript::Japanese
    } else {
        CjkScript::TraditionalChinese
    }
}

/// 標準字型目錄（依 macOS 慣例）。
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/System/Library/Fonts/Supplemental"),
        PathBuf::from("/Library/Fonts"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(Path::new(&home).join("Library/Fonts"));
    }
    dirs
}

/// 掃描字型目錄，回傳所有檔名（小寫）。
pub fn installed_files() -> Vec<String> {
    let mut names = Vec::new();
    for dir in font_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_ascii_lowercase());
            }
        }
    }
    names
}

/// 從檔名清單與指定候選表挑出第一個可用的候選字型。
fn pick_in_table(table: &[(&'static str, &[&str])], files: &[String]) -> Option<&'static str> {
    for (family, tokens) in table {
        let found = files.iter().any(|file| {
            tokens
                .iter()
                .any(|token| file.replace([' ', '_'], "").contains(token))
        });
        if found {
            return Some(family);
        }
    }
    None
}

/// 從檔名清單挑出第一個可用的候選字型。
///
/// 抽成純函式以便測試（不需要真的安裝字型）。
pub fn pick_from(files: &[String]) -> Option<&'static str> {
    pick_in_table(CANDIDATES, files)
}

/// 依東亞語系偏好從檔名清單挑出第一個可用的候選字型；若該語系無可用字型則退回其他東亞候選。
pub fn pick_from_for(script: CjkScript, files: &[String]) -> Option<&'static str> {
    let tables: [&[(&'static str, &[&str])]; 3] = match script {
        CjkScript::TraditionalChinese => [CANDIDATES, CANDIDATES_JP, CANDIDATES_SC],
        CjkScript::SimplifiedChinese => [CANDIDATES_SC, CANDIDATES, CANDIDATES_JP],
        CjkScript::Japanese => [CANDIDATES_JP, CANDIDATES, CANDIDATES_SC],
    };
    tables
        .into_iter()
        .find_map(|table| pick_in_table(table, files))
}

/// 挑選目前系統可用的東亞字型。
pub fn pick_east_asian() -> Option<&'static str> {
    pick_from(&installed_files())
}

/// 依語系偏好挑選目前系統可用的東亞字型。
pub fn pick_east_asian_for(script: CjkScript) -> Option<&'static str> {
    pick_from_for(script, &installed_files())
}

/// 回傳每個候選字型的可用狀態，供 `n2p fonts` 顯示。
pub fn availability() -> Vec<(&'static str, bool)> {
    let files = installed_files();
    CANDIDATES
        .iter()
        .map(|(family, tokens)| {
            let found = files.iter().any(|file| {
                tokens
                    .iter()
                    .any(|token| file.replace([' ', '_'], "").contains(token))
            });
            (*family, found)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_ascii_lowercase()).collect()
    }

    #[test]
    fn picks_noto_first() {
        let installed = files(&["Helvetica.ttc", "NotoSansCJKtc-Regular.otf", "PingFang.ttc"]);
        assert_eq!(pick_from(&installed), Some("Noto Sans TC"));
    }

    #[test]
    fn falls_back_in_priority_order() {
        assert_eq!(pick_from(&files(&["PingFang.ttc"])), Some("PingFang TC"));
        assert_eq!(pick_from(&files(&["STHeiti Light.ttc"])), Some("Heiti TC"));
        assert_eq!(
            pick_from(&files(&["SourceHanSansTC-Regular.otf"])),
            Some("Source Han Sans TC")
        );
    }

    #[test]
    fn picks_japanese_and_simplified_chinese_fonts_by_script() {
        let jp_installed = files(&["ヒラギノ角ゴシック W3.ttc", "STHeiti Medium.ttc"]);
        assert_eq!(
            pick_from_for(CjkScript::Japanese, &jp_installed),
            Some("Hiragino Sans")
        );
        let noto_jp = files(&["NotoSansCJKjp-Regular.otf", "ヒラギノ角ゴシック W3.ttc"]);
        assert_eq!(
            pick_from_for(CjkScript::Japanese, &noto_jp),
            Some("Noto Sans JP")
        );
        let sc_installed = files(&["NotoSansCJKsc-Regular.otf", "PingFang.ttc"]);
        assert_eq!(
            pick_from_for(CjkScript::SimplifiedChinese, &sc_installed),
            Some("Noto Sans SC")
        );
    }

    #[test]
    fn detects_japanese_kana_in_slide_text() {
        assert_eq!(
            detect_script("日本語のプレゼンテーション"),
            CjkScript::Japanese
        );
        assert_eq!(detect_script("スライド変換"), CjkScript::Japanese);
        assert_eq!(
            detect_script("提升學生正向幸福"),
            CjkScript::TraditionalChinese
        );
    }

    #[test]
    fn returns_none_when_nothing_matches() {
        assert_eq!(pick_from(&files(&["Helvetica.ttc", "Arial.ttf"])), None);
        assert_eq!(pick_from(&[]), None);
    }

    #[test]
    fn handles_spaces_and_underscores_in_filenames() {
        assert_eq!(
            pick_from(&files(&["Noto Sans TC Regular.otf"])),
            Some("Noto Sans TC")
        );
        assert_eq!(
            pick_from(&files(&["NotoSans_TC.otf"])),
            Some("Noto Sans TC")
        );
    }

    #[test]
    fn detects_this_machine_without_panicking() {
        // 只驗證不會爆；實際裝了什麼由環境決定。
        let picked = pick_east_asian();
        let listing = availability();
        assert_eq!(listing.len(), CANDIDATES.len());
        if let Some(family) = picked {
            assert!(listing.iter().any(|(f, ok)| *f == family && *ok));
        }
    }
}
