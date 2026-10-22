//! 相容性基準清單。
//!
//! 記錄「PowerPoint 開啟某個 pptx 之後讀到什麼」，用來偵測我們的產生器是否
//! 在某次修改後寫出 PowerPoint 讀不懂（或讀錯）的檔案。
//!
//! 只存**可比較的觀測值**：頁數、畫布尺寸、每頁的形狀名稱與文字。刻意不存
//! 字型、顏色、z 序等細節 —— 這個清單要抓的是「結構壞掉」，不是「像素不同」。

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::powerpoint::{PresentationFacts, normalize_whitespace};

/// 整份基準清單。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    /// 產生這份清單的後端。
    #[serde(default = "default_backend")]
    pub backend: String,
    pub entries: Vec<Entry>,
}

fn default_backend() -> String {
    "powerpoint".to_owned()
}

/// 一個檔案的基準。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub slides: usize,
    /// 畫布寬度（點）。高度無法透過 macOS AppleScript 取得（見 `powerpoint` 模組說明）。
    pub width_pt: f64,
    pub pages: Vec<PageEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageEntry {
    pub shape_names: Vec<String>,
    pub texts: Vec<String>,
}

impl Entry {
    pub fn from_facts(name: String, facts: PresentationFacts) -> Self {
        Self {
            name,
            slides: facts.slides,
            width_pt: facts.width_pt,
            pages: facts
                .pages
                .into_iter()
                .map(|page| PageEntry {
                    shape_names: page.shape_names,
                    texts: page
                        .texts
                        .into_iter()
                        .map(|t| normalize_whitespace(&t))
                        .collect(),
                })
                .collect(),
        }
    }

    /// 比對，回傳第一個差異的說明。
    pub fn compare(&self, actual: &Entry) -> Result<(), String> {
        if self.slides != actual.slides {
            return Err(format!("頁數 {}，基準 {} ", actual.slides, self.slides));
        }
        if (self.width_pt - actual.width_pt).abs() > 0.5 {
            return Err(format!(
                "畫布寬 {:.1} pt，基準 {:.1} pt",
                actual.width_pt, self.width_pt
            ));
        }
        if self.pages.len() != actual.pages.len() {
            return Err(format!(
                "頁面紀錄 {} 筆，基準 {} 筆",
                actual.pages.len(),
                self.pages.len()
            ));
        }
        for (index, (want, got)) in self.pages.iter().zip(actual.pages.iter()).enumerate() {
            if want.shape_names != got.shape_names {
                return Err(format!(
                    "第 {} 頁形狀 {:?}，基準 {:?}",
                    index + 1,
                    got.shape_names,
                    want.shape_names
                ));
            }
            if want.texts.len() != got.texts.len() {
                return Err(format!(
                    "第 {} 頁文字框 {} 個，基準 {} 個",
                    index + 1,
                    got.texts.len(),
                    want.texts.len()
                ));
            }
            for (slot, (want_text, got_text)) in want.texts.iter().zip(got.texts.iter()).enumerate()
            {
                if normalize_whitespace(want_text) != normalize_whitespace(got_text) {
                    return Err(format!(
                        "第 {} 頁第 {} 個文字框內容不同",
                        index + 1,
                        slot + 1
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("讀不到 {}", path.display()))?;
        let manifest: Manifest = serde_json::from_str(&text)
            .with_context(|| format!("{} 不是合法的基準清單", path.display()))?;
        if manifest.entries.is_empty() {
            return Err(anyhow!("{} 沒有任何項目", path.display()));
        }
        Ok(manifest)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut sorted = self.clone();
        sorted.entries.sort_by(|a, b| a.name.cmp(&b.name));
        let text = serde_json::to_string_pretty(&sorted)?;
        std::fs::write(path, format!("{text}\n"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        Entry {
            name: "sample".into(),
            slides: 1,
            width_pt: 960.0,
            pages: vec![PageEntry {
                shape_names: vec!["Block_text".into()],
                texts: vec!["hello world".into()],
            }],
        }
    }

    #[test]
    fn identical_entries_match() {
        assert!(entry().compare(&entry()).is_ok());
    }

    #[test]
    fn slide_count_mismatch_is_reported() {
        let mut actual = entry();
        actual.slides = 2;
        let message = entry().compare(&actual).unwrap_err();
        assert!(message.contains("頁數"), "{message}");
    }

    #[test]
    fn size_mismatch_is_reported() {
        let mut actual = entry();
        actual.width_pt = 720.0;
        assert!(entry().compare(&actual).unwrap_err().contains("畫布"));
    }

    #[test]
    fn shape_name_mismatch_is_reported() {
        let mut actual = entry();
        actual.pages[0].shape_names = vec!["Other".into()];
        assert!(entry().compare(&actual).unwrap_err().contains("形狀"));
    }

    #[test]
    fn whitespace_differences_are_ignored() {
        let mut actual = entry();
        actual.pages[0].texts = vec!["hello   world".into()];
        assert!(entry().compare(&actual).is_ok());
    }

    #[test]
    fn text_content_mismatch_is_reported() {
        let mut actual = entry();
        actual.pages[0].texts = vec!["hello there".into()];
        assert!(entry().compare(&actual).unwrap_err().contains("文字框內容"));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("n2p-xtask-manifest");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest.json");
        let manifest = Manifest {
            backend: "powerpoint".into(),
            entries: vec![entry()],
        };
        manifest.save(&path).unwrap();
        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].name, "sample");
    }

    #[test]
    fn empty_manifest_is_rejected() {
        let dir = std::env::temp_dir().join("n2p-xtask-manifest");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.json");
        std::fs::write(&path, r#"{"backend":"powerpoint","entries":[]}"#).unwrap();
        assert!(Manifest::load(&path).is_err());
    }
}
