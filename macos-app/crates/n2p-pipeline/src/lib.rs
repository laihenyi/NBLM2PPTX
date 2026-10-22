//! 串接整條流程。
//!
//! ```text
//! PDF ──n2p-pdf──▶ 頁面影像 ──n2p-image──▶ 去水印
//!                        │
//!                        ├──n2p-vision──┐
//!                        └──n2p-mineru──┴──▶ 版面模型 ──n2p-build──▶ PPTX
//! ```
//!
//! 每一步的產物都留在工作目錄，方便中斷後重跑與除錯（與 Python 版一致）。

use std::path::{Path, PathBuf};

use n2p_build::{BuildOptions, PageInput};
use n2p_core::RgbImage;
use n2p_image::InpaintMethod;
use n2p_layout::{LayoutPage, LayoutProvider};
use n2p_mineru::MineruDocument;
use n2p_vision::{VisionConfig, VisionDocument};

/// 錯誤型別。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("PDF 處理失敗：{0}")]
    Pdf(#[from] n2p_pdf::Error),

    #[error("影像處理失敗：{0}")]
    Image(String),

    #[error("版面解析失敗：{0}")]
    Layout(String),

    #[error("PPTX 產生失敗：{0}")]
    Build(#[from] n2p_build::Error),

    #[error("OOXML 操作失敗：{0}")]
    Ooxml(#[from] n2p_ooxml::Error),

    #[error("版面模型錯誤：{0}")]
    LayoutModel(#[from] n2p_layout::Error),

    #[error("I/O 失敗：{0}")]
    Io(#[from] std::io::Error),

    #[error("設定錯誤：{0}")]
    Config(String),

    #[error("已取消")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, Error>;

/// 版面來源。
#[derive(Debug, Clone)]
pub enum LayoutSource {
    /// 不用版面資訊：每頁就是一張滿版圖片。
    None,
    /// macOS Vision OCR。
    Vision(VisionConfig),
    /// MinerU／Paddle JSON。
    Mineru(PathBuf),
    /// 平台支援時用 Vision，否則退回 `None`。
    Auto,
}

/// 轉換選項。
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub dpi: f64,
    pub inpaint: bool,
    pub inpaint_method: InpaintMethod,
    /// 是否把頁面調整成 16:9。
    pub widescreen: bool,
    pub layout: LayoutSource,
    pub build: BuildOptions,
    /// 只處理指定頁（1 起算）。
    pub pages: Option<Vec<usize>>,
    /// 中間產物目錄；`None` 表示不落檔。
    pub work_dir: Option<PathBuf>,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            dpi: 150.0,
            inpaint: true,
            inpaint_method: InpaintMethod::default(),
            widescreen: false,
            layout: LayoutSource::Auto,
            build: BuildOptions::default(),
            pages: None,
            work_dir: None,
        }
    }
}

/// 轉換結果。
#[derive(Debug, Clone)]
pub struct ConvertReport {
    pub pages: usize,
    pub slide_size: n2p_ooxml::SlideSize,
    pub layout_name: String,
    pub text_boxes: usize,
    pub pictures: usize,
    pub watermarks_removed: usize,
    pub output_bytes: u64,
}

/// 轉換進度。頁序皆為 0 起算的「第幾個處理中的頁面」，`total` 是本次處理的頁數。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// 點陣化與去水印。
    Render { page: usize, total: usize },
    /// 版面辨識（一次處理全部頁面）。
    Layout,
    /// 產生投影片（擦字、切物件）。
    Build { page: usize, total: usize },
    /// 寫出 PPTX。
    Save,
}

/// 把 PDF 轉成 PPTX。
pub fn convert(
    pdf_path: &Path,
    out_path: &Path,
    options: &ConvertOptions,
) -> Result<ConvertReport> {
    convert_with_progress(pdf_path, out_path, options, &mut |_| true)
}

/// 同 [`convert`]，並在每個階段回報進度。回呼回傳 `false` 時盡快中止，
/// 回傳 [`Error::Cancelled`]，不會寫出輸出檔。
pub fn convert_with_progress(
    pdf_path: &Path,
    out_path: &Path,
    options: &ConvertOptions,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<ConvertReport> {
    if options.dpi <= 0.0 {
        return Err(Error::Config(format!(
            "DPI 必須大於 0，實際 {}",
            options.dpi
        )));
    }

    let doc = n2p_pdf::PdfDoc::open(pdf_path)?;
    let total = doc.page_count();
    if total == 0 {
        return Err(Error::Config("PDF 沒有任何頁面".into()));
    }

    // ── 1. 點陣化 + 去水印 ────────────────────────────────────────────────
    let selected: Vec<usize> = match &options.pages {
        Some(pages) => {
            let mut kept: Vec<usize> = pages
                .iter()
                .copied()
                .filter(|p| *p >= 1 && *p <= total)
                .collect();
            kept.sort_unstable();
            if kept.is_empty() {
                return Err(Error::Config(format!("指定的頁碼都不在 1..={total} 之內")));
            }
            kept
        }
        None => (1..=total).collect(),
    };

    if let Some(dir) = &options.work_dir {
        std::fs::create_dir_all(dir)?;
    }

    let mut images: Vec<RgbImage> = Vec::with_capacity(selected.len());
    let mut page_sizes_pt: Vec<(f64, f64)> = Vec::with_capacity(selected.len());
    let mut watermarks_removed = 0usize;

    for (slot, page_num) in selected.iter().enumerate() {
        if !progress(Progress::Render {
            page: slot,
            total: selected.len(),
        }) {
            return Err(Error::Cancelled);
        }
        let index = page_num - 1;
        let mut image = doc.render_rgb(index, options.dpi)?;
        let size_pt = doc.page_size_pt(index)?;

        if options.inpaint {
            match n2p_image::inpaint_watermark(&image, options.inpaint_method) {
                Ok(cleaned) => {
                    image = cleaned;
                    watermarks_removed += 1;
                }
                // 尺寸不符合 NotebookLM 基準時略過去水印，而不是讓整個轉換失敗
                Err(n2p_image::watermark::Error::AspectMismatch { .. }) => {}
            }
        }

        let (image, size_pt) = if options.widescreen {
            widen_to_16x9(image, size_pt)
        } else {
            (image, size_pt)
        };

        if let Some(dir) = &options.work_dir {
            let path = dir.join(format!("page_{page_num:04}.png"));
            image.save(&path).map_err(|e| Error::Image(e.to_string()))?;
        }

        page_sizes_pt.push(size_pt);
        images.push(image);
    }

    // ── 2. 版面 ───────────────────────────────────────────────────────────
    if !progress(Progress::Layout) {
        return Err(Error::Cancelled);
    }
    let (layout_name, layouts) =
        resolve_layout(options, &images, &page_sizes_pt, &selected, total)?;

    // ── 3. 產生簡報 ───────────────────────────────────────────────────────
    let inputs: Vec<PageInput<'_>> = layouts
        .iter()
        .zip(images.iter())
        .map(|(layout, image)| PageInput { layout, image })
        .collect();

    let total_pages = inputs.len();
    let (deck, report) =
        n2p_build::build_deck_with_progress(&inputs, &options.build, &mut |page| {
            progress(Progress::Build {
                page,
                total: total_pages,
            })
        })
        .map_err(|e| match e {
            n2p_build::Error::Cancelled => Error::Cancelled,
            other => Error::Build(other),
        })?;
    if !progress(Progress::Save) {
        return Err(Error::Cancelled);
    }
    deck.save(out_path)?;

    Ok(ConvertReport {
        pages: images.len(),
        slide_size: report.slide_size,
        layout_name,
        text_boxes: report.pages.iter().map(|p| p.text_boxes).sum(),
        pictures: report.pages.iter().map(|p| p.pictures).sum(),
        watermarks_removed,
        output_bytes: std::fs::metadata(out_path).map(|m| m.len()).unwrap_or(0),
    })
}

/// 釋放 LaMa 與 SAM 的共用引擎。
///
/// 引擎與推論暫存會一直留在行程裡（轉換後 RSS 仍有 5–7 GB）。命令列每次都是新行程，
/// 不受影響；長駐的 GUI 應在每個工作結束後呼叫。下一次轉換會自動重新載入。
pub fn release_models() {
    n2p_image::lama::release_shared();
    n2p_image::sam::release_shared();
}

/// 解析 `1-3,5` 這種頁碼範圍（1 起算）。
pub fn parse_page_range(text: &str) -> Result<Vec<usize>> {
    let bad = |part: &str| Error::Config(format!("頁碼範圍不合法：{part}"));
    let mut pages = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.split_once('-') {
            Some((start, end)) => {
                let start: usize = start.trim().parse().map_err(|_| bad(part))?;
                let end: usize = end.trim().parse().map_err(|_| bad(part))?;
                if start == 0 || end < start {
                    return Err(bad(part));
                }
                pages.extend(start..=end);
            }
            None => {
                let page: usize = part.parse().map_err(|_| bad(part))?;
                if page == 0 {
                    return Err(Error::Config(format!("頁碼從 1 開始：{part}")));
                }
                pages.push(page);
            }
        }
    }
    if pages.is_empty() {
        return Err(Error::Config("頁碼範圍是空的".into()));
    }
    Ok(pages)
}

/// 把頁面在**圖片層**補成 16:9，並同步更新頁面尺寸（點）。
///
/// 與 Windows 原版的 `pdf2png.make_wide_screen` 一致：比例在產生 PPT 之前就處理好，
/// 所以投影片不需要拉伸，圓形不會變橢圓。
///
/// **尺寸必須同步更新**，否則 `SlideSize::from_ratio` 會拿到舊的 4:3 比例，
/// 於是 16:9 的圖片被塞進 4:3 畫布 —— 整體被水平拉伸。
fn widen_to_16x9(image: RgbImage, size_pt: (f64, f64)) -> (RgbImage, (f64, f64)) {
    if n2p_image::is_widescreen(&image) || size_pt.0 <= 0.0 {
        return (image, size_pt);
    }
    // 先記下「每點幾個像素」，補白後用它換算出新的頁面寬度
    let pixels_per_pt = image.width as f64 / size_pt.0;
    let widened = n2p_image::to_widescreen(&image);
    let new_width_pt = widened.width as f64 / pixels_per_pt;
    (widened, (new_width_pt, size_pt.1))
}

fn resolve_layout(
    options: &ConvertOptions,
    images: &[RgbImage],
    page_sizes_pt: &[(f64, f64)],
    selected: &[usize],
    total: usize,
) -> Result<(String, Vec<LayoutPage>)> {
    match &options.layout {
        LayoutSource::None => Ok((
            "none".to_owned(),
            page_sizes_pt
                .iter()
                .map(|size| LayoutPage {
                    size_pt: *size,
                    blocks: Vec::new(),
                })
                .collect(),
        )),

        LayoutSource::Vision(config) => {
            let document = VisionDocument::from_images(images, page_sizes_pt, config)
                .map_err(|e| Error::Layout(e.to_string()))?;
            Ok(("vision".to_owned(), document.pages()?))
        }

        LayoutSource::Mineru(path) => {
            let document = MineruDocument::open(path).map_err(|e| Error::Layout(e.to_string()))?;
            if document.page_count() < total {
                return Err(Error::Layout(format!(
                    "JSON 只有 {} 頁，PDF 有 {total} 頁",
                    document.page_count()
                )));
            }
            // 只取實際處理的頁，並在需要時平移到寬屏化座標
            let mut pages = Vec::with_capacity(images.len());
            for (slot, page_num) in selected.iter().enumerate() {
                let mut page = document
                    .page(page_num - 1)
                    .map_err(|e| Error::Layout(e.to_string()))?;
                if options.widescreen {
                    let original_width_pt = page.size_pt.0;
                    let new_width_pt = page_sizes_pt[slot].0;
                    let offset = (new_width_pt - original_width_pt) / 2.0;
                    page = page.translated_x(offset, new_width_pt);
                }
                pages.push(page);
            }
            Ok(("mineru".to_owned(), pages))
        }

        LayoutSource::Auto => {
            let vision = VisionConfig::default();
            match VisionDocument::from_images(images, page_sizes_pt, &vision) {
                Ok(document) => Ok(("vision".to_owned(), document.pages()?)),
                Err(_) => resolve_layout(
                    &ConvertOptions {
                        layout: LayoutSource::None,
                        ..options.clone()
                    },
                    images,
                    page_sizes_pt,
                    selected,
                    total,
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn example_pdf() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/floyd.pdf")
    }

    fn temp_out(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("n2p-pipeline-tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn widescreen_updates_page_size_so_the_slide_aspect_follows_the_image() {
        // 4:3 頁面（400x300 pt）→ 補成 16:9
        let image = RgbImage::filled(800, 600, [200, 220, 240]); // 每點 2 px
        let (widened, size_pt) = widen_to_16x9(image, (400.0, 300.0));

        assert_eq!(widened.height, 600, "高度不該變");
        assert_eq!(widened.width, 1067, "600 * 16/9 = 1066.67 → 1067 px");
        // 每點 2 px → 新頁寬 1067/2 = 533.5 pt，533.5/300 = 1.778（16:9）
        let _ = size_pt;
        assert!(
            (size_pt.0 / size_pt.1 - widened.width as f64 / widened.height as f64).abs() < 0.01,
            "頁面比例應與補白後的圖片一致：{:?} vs {}x{}",
            size_pt,
            widened.width,
            widened.height
        );

        // 補白處是白色、中央保留原內容
        assert_eq!(widened.pixel(0, 0), [255, 255, 255]);
        assert_eq!(widened.pixel(widened.width / 2, 300), [200, 220, 240]);
    }

    #[test]
    fn already_widescreen_pages_are_left_alone() {
        let image = RgbImage::filled(1600, 900, [10, 20, 30]);
        let (widened, size_pt) = widen_to_16x9(image.clone(), (800.0, 450.0));
        assert_eq!(widened, image);
        assert_eq!(size_pt, (800.0, 450.0));
    }

    #[test]
    fn page_ranges_are_parsed() {
        assert_eq!(parse_page_range("1-3,5").unwrap(), vec![1, 2, 3, 5]);
        assert_eq!(parse_page_range(" 2 ").unwrap(), vec![2]);
        assert_eq!(parse_page_range("4-4").unwrap(), vec![4]);
        for bad in ["0-2", "3-1", "abc", "", "0"] {
            assert!(parse_page_range(bad).is_err(), "{bad:?} 應該被拒絕");
        }
    }

    #[test]
    fn progress_reports_every_stage_in_order() {
        let options = ConvertOptions {
            layout: LayoutSource::None,
            inpaint: false,
            ..Default::default()
        };
        let mut seen = Vec::new();
        convert_with_progress(
            &example_pdf(),
            &temp_out("progress.pptx"),
            &options,
            &mut |p| {
                seen.push(p);
                true
            },
        )
        .unwrap();
        assert_eq!(
            seen,
            vec![
                Progress::Render { page: 0, total: 2 },
                Progress::Render { page: 1, total: 2 },
                Progress::Layout,
                Progress::Build { page: 0, total: 2 },
                Progress::Build { page: 1, total: 2 },
                Progress::Save,
            ]
        );
    }

    #[test]
    fn cancelling_stops_without_writing_output() {
        let options = ConvertOptions {
            layout: LayoutSource::None,
            inpaint: false,
            ..Default::default()
        };
        for stop_at in [
            Progress::Render { page: 1, total: 2 },
            Progress::Layout,
            Progress::Build { page: 1, total: 2 },
            Progress::Save,
        ] {
            let out = temp_out("cancelled.pptx");
            let _ = std::fs::remove_file(&out);
            let error =
                convert_with_progress(&example_pdf(), &out, &options, &mut |p| p != stop_at)
                    .unwrap_err();
            assert!(matches!(error, Error::Cancelled), "{stop_at:?}：{error}");
            assert!(!out.exists(), "{stop_at:?}：取消後不該寫出檔案");
        }
    }

    #[test]
    fn rejects_bad_dpi() {
        let options = ConvertOptions {
            dpi: 0.0,
            ..Default::default()
        };
        let error = convert(&example_pdf(), &temp_out("x.pptx"), &options).unwrap_err();
        assert!(error.to_string().contains("DPI"));
    }

    #[test]
    fn image_only_conversion_produces_one_slide_per_page() {
        let options = ConvertOptions {
            layout: LayoutSource::None,
            inpaint: false,
            ..Default::default()
        };
        let out = temp_out("image_only.pptx");
        let report = convert(&example_pdf(), &out, &options).unwrap();

        assert_eq!(report.pages, 2);
        assert_eq!(report.layout_name, "none");
        assert_eq!(report.text_boxes, 0);
        assert!(report.output_bytes > 0);

        let deck = n2p_ooxml::Deck::open(&out).unwrap();
        assert_eq!(deck.slide_count().unwrap(), 2);
        for index in 0..2 {
            let doc = deck.slide(index).unwrap();
            assert!(
                n2p_ooxml::slide::background(&doc).is_some(),
                "第 {index} 頁應有背景"
            );
        }
    }

    #[test]
    fn page_selection_is_one_based() {
        let options = ConvertOptions {
            layout: LayoutSource::None,
            inpaint: false,
            pages: Some(vec![2]),
            ..Default::default()
        };
        let report = convert(&example_pdf(), &temp_out("page2.pptx"), &options).unwrap();
        assert_eq!(report.pages, 1);
    }

    #[test]
    fn out_of_range_page_selection_is_rejected() {
        let options = ConvertOptions {
            pages: Some(vec![9]),
            layout: LayoutSource::None,
            ..Default::default()
        };
        let error = convert(&example_pdf(), &temp_out("bad.pptx"), &options).unwrap_err();
        assert!(matches!(error, Error::Config(_)));
    }

    #[test]
    fn work_dir_keeps_page_images() {
        let dir = std::env::temp_dir().join("n2p-pipeline-tests").join("work");
        let _ = std::fs::remove_dir_all(&dir);
        let options = ConvertOptions {
            layout: LayoutSource::None,
            inpaint: false,
            work_dir: Some(dir.clone()),
            ..Default::default()
        };
        convert(&example_pdf(), &temp_out("work.pptx"), &options).unwrap();
        assert!(dir.join("page_0001.png").exists());
        assert!(dir.join("page_0002.png").exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn vision_layout_adds_text_boxes() {
        let options = ConvertOptions {
            layout: LayoutSource::Vision(VisionConfig::default()),
            inpaint: false,
            ..Default::default()
        };
        let report = convert(&example_pdf(), &temp_out("vision.pptx"), &options).unwrap();
        assert_eq!(report.layout_name, "vision");
        assert!(report.text_boxes > 0, "Vision 應辨識出文字");
        println!(
            "vision: {} 頁、{} 個文字框、{} 張圖片、{} bytes",
            report.pages, report.text_boxes, report.pictures, report.output_bytes
        );
    }
}
