//! LibreOffice 後端：pptx → PDF → PNG，用於像素級的視覺回歸。
//!
//! 這是唯一能回答「看起來一樣嗎」的路徑。PowerPoint 的 AppleScript 沒有匯出 PDF 的
//! 命令（見 [`crate::powerpoint`]），所以視覺比對要靠 LibreOffice。

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use n2p_core::RgbImage;

/// 常見的 `soffice` 位置。
const CANDIDATES: [&str; 5] = [
    "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    "/opt/homebrew/bin/soffice",
    "/usr/local/bin/soffice",
    "/usr/bin/soffice",
    "/snap/bin/soffice",
];

/// 找出可用的 `soffice`。
pub fn find() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("SOFFICE_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    for candidate in CANDIDATES {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Some(path);
        }
    }
    // 最後試 PATH
    for dir in std::env::var("PATH").unwrap_or_default().split(':') {
        let candidate = Path::new(dir).join("soffice");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// 回傳 `soffice --version` 的第一行。
pub fn version(binary: &Path) -> Result<String> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .context("執行 soffice --version 失敗")?;
    if !output.status.success() {
        return Err(anyhow!(
            "soffice --version 失敗：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned())
}

/// 把 pptx 轉成 PDF，回傳輸出路徑。
///
/// 用一個獨立的 `-env:UserInstallation` 目錄，避免與使用者正在開著的
/// LibreOffice 互相干擾（這是 `soffice --headless` 常見的卡住原因）。
pub fn to_pdf(binary: &Path, pptx: &Path, out_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)?;
    let profile = out_dir.join(".lo-profile");
    std::fs::create_dir_all(&profile)?;

    // 先複製成安全的 ASCII 檔名再轉。
    // 真實檔名常含空格與中文（例如「CH9_用 2.0 版的愛…_可編輯.pptx」），
    // soffice 對這種路徑會靜默不產生輸出 —— 但它仍回傳 exit 0。
    let safe_name = format!("input-{}.pptx", short_hash(pptx));
    let safe_input = out_dir.join(&safe_name);
    std::fs::copy(pptx, &safe_input)
        .with_context(|| format!("無法複製 {}", pptx.display()))?;

    let output = Command::new(binary)
        .arg("--headless")
        .arg("--norestore")
        .arg(format!(
            "-env:UserInstallation=file://{}",
            profile.display()
        ))
        .arg("--convert-to")
        .arg("pdf")
        .arg("--outdir")
        .arg(out_dir)
        .arg(&safe_input)
        .output()
        .context("執行 soffice 轉檔失敗")?;

    if !output.status.success() {
        return Err(anyhow!(
            "soffice 轉檔失敗（{}）：{}",
            pptx.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let pdf = out_dir.join(&safe_name).with_extension("pdf");
    if !pdf.exists() {
        return Err(anyhow!(
            "soffice 沒有產出 {}（來源：{}）",
            pdf.display(),
            pptx.display()
        ));
    }
    let _ = std::fs::remove_file(&safe_input);
    Ok(pdf)
}

/// 取路徑的短雜湊，用來產生不衝突的安全檔名。
fn short_hash(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:08x}", hasher.finish() as u32)
}

/// 把 PDF 每一頁點陣化成 RGB（用本專案自己的 `n2p-pdf`）。
pub fn pdf_pages_to_images(pdf: &Path, dpi: f64) -> Result<Vec<RgbImage>> {
    let doc = n2p_pdf::PdfDoc::open(pdf)?;
    let mut pages = Vec::with_capacity(doc.page_count());
    for index in 0..doc.page_count() {
        pages.push(doc.render_rgb(index, dpi)?);
    }
    Ok(pages)
}

/// 便利函式：pptx → 每頁 RGB。
pub fn pptx_to_images(
    binary: &Path,
    pptx: &Path,
    work_dir: &Path,
    dpi: f64,
) -> Result<Vec<RgbImage>> {
    let pdf = to_pdf(binary, pptx, work_dir)?;
    pdf_pages_to_images(&pdf, dpi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_returns_none_or_existing_path() {
        // 本機沒有 LibreOffice 是正常情況，因此不要求必須存在。
        if let Some(path) = find() {
            assert!(path.is_file(), "{path:?} 應存在");
        }
    }
}
