//! 版面模型 + 頁面影像 → PPTX。
//!
//! 移植自 `ppt_creater.py`（`create_ppt_from_paddle_json` 的主流程）與
//! `ppt_refiner.py` 的背景處理段落。
//!
//! ## 每一頁的組成
//!
//! ```text
//! 1. 建立一頁（從內嵌的空白範本複製，共用同一套 master/layout/theme）
//! 2. 文字區塊 → 文字框（依 bbox 高度推算字級、依 label 決定對齊與留白）
//! 3. 圖片/表格/圖表區塊 → 從頁面影像裁切 → 置底的圖片形狀
//! 4. 擦除已轉成文字框或圖片的區域 → 剩下的影像當投影片背景
//! ```
//!
//! 第 2 步先做、第 3 步置底，因此視覺層序是：背景 < 圖片 < 文字。
//!
//! ## 與 Python 版的刻意差異
//!
//! * **不需要 `clean_ppt`**：自己產生 XML，不會有 Spire 的評估水印。
//! * **背景解析度可選**：Python 版把頁面 PNG 縮到「點數尺寸」（例如 2867×1600
//!   縮成 1376×768）再當背景，等於丟掉一半解析度。這裡預設維持原行為以求等價，
//!   但可以用 [`BackgroundResolution::FullImage`] 保留完整解析度。

use n2p_core::{BBox, RgbImage};
use n2p_image::edge_diversity;
use n2p_layout::{Block, BlockKind, LayoutPage};
use n2p_ooxml::merge;
use n2p_ooxml::{Align, AutoFit, Deck, Package, Rect, SlideSize, TextBox};

/// 內嵌的空白簡報範本。
///
/// 由 Spire.Presentation 產生後移除所有形狀與評估水印，只留下 master/layout/theme。
/// 27 KB，含 11 個版面配置。
pub const BLANK_TEMPLATE: &[u8] = include_bytes!("../assets/blank.pptx");

/// 文字框的垂直位移（`delta_y = 2`）。
const TEXT_DELTA_Y_PT: f64 = 2.0;
/// 一般文字的垂直留白。
const V_PADDING_PT: f64 = 5.0;
/// 一般文字的水平留白（Python 版是 `h_padding + h_padding`，各 15）。
const H_PADDING_BODY_PT: f64 = 15.0;
/// 標題的水平留白。
const H_PADDING_TITLE_PT: f64 = 5.0;
/// 最小字級。
const MIN_FONT_SIZE_PT: f64 = 8.0;
/// 字級相對 bbox 高度的比例（`font_size = height * 0.75`）。
const FONT_HEIGHT_RATIO: f64 = 0.75;
/// 背景擦除時用的顏色容差（`tolerance=20`）。
const ERASE_TOLERANCE: u8 = 20;
/// 前景圖片向外擴張的像素數（`expand_px=2`）。
const FOREGROUND_EXPAND_PX: i64 = 2;

/// 錯誤型別。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("PPTX 操作失敗：{0}")]
    Ooxml(#[from] n2p_ooxml::Error),

    #[error("影像操作失敗：{0}")]
    Image(String),

    #[error("沒有可用的頁面")]
    NoPages,

    #[error("頁面尺寸不合法：{0}")]
    BadPageSize(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// 背景圖的解析度策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackgroundResolution {
    /// 縮到頁面點數尺寸。與 Python 版等價，但會丟失解析度。
    #[default]
    PageSize,
    /// 保留原始點陣化解析度（檔案較大，但背景更清晰）。
    FullImage,
}

/// 產生選項。
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub font_name: String,
    /// 是否把頁面調整成 16:9。
    pub widescreen: bool,
    /// 是否在背景圖上擦除已轉成文字/圖片的區域。
    pub erase_background_regions: bool,
    pub background_resolution: BackgroundResolution,
    /// 文字框是否寫入 `<a:normAutofit/>`。
    ///
    /// Python 版（經 Spire）不會寫，因此溢出時文字會超出框外；開啟可修好這件事。
    pub text_autofit: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            font_name: "Calibri".to_owned(),
            widescreen: false,
            erase_background_regions: true,
            background_resolution: BackgroundResolution::PageSize,
            text_autofit: false,
        }
    }
}

/// 一頁的輸入。
pub struct PageInput<'a> {
    pub layout: &'a LayoutPage,
    /// 已去水印（或未去水印）的頁面影像，像素座標。
    pub image: &'a RgbImage,
}

/// 單頁建置的統計，方便診斷「為什麼某頁沒有文字」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageReport {
    pub text_boxes: usize,
    pub skipped_text: usize,
    pub pictures: usize,
}

/// 整體建置結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub pages: Vec<PageReport>,
    pub slide_size: SlideSize,
}

impl Default for BuildReport {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            slide_size: SlideSize::Screen4x3,
        }
    }
}

/// 依版面與影像產生整份簡報。
pub fn build_deck(inputs: &[PageInput<'_>], options: &BuildOptions) -> Result<(Deck, BuildReport)> {
    if inputs.is_empty() {
        return Err(Error::NoPages);
    }

    // 空白範本：只留一頁，其餘頁面靠附加複製。
    let template = Deck::from_package(Package::from_bytes(BLANK_TEMPLATE).map_err(Error::Ooxml)?)
        .map_err(Error::Ooxml)?;
    let mut deck = template.clone();
    deck.keep_only_slide(0).map_err(Error::Ooxml)?;

    let first = &inputs[0];
    let slide_size = SlideSize::from_ratio(first.layout.size_pt.0, first.layout.size_pt.1);
    deck.set_slide_size(slide_size).map_err(Error::Ooxml)?;

    let mut report = BuildReport {
        pages: Vec::with_capacity(inputs.len()),
        slide_size,
    };

    for (index, input) in inputs.iter().enumerate() {
        if index > 0 {
            merge::append_slide(&mut deck, &template, 0).map_err(Error::Ooxml)?;
        }
        let page_report = build_page(&mut deck, index, input, &slide_size, options)?;
        report.pages.push(page_report);
    }

    deck.sync_app_properties().map_err(Error::Ooxml)?;
    Ok((deck, report))
}

fn build_page(
    deck: &mut Deck,
    index: usize,
    input: &PageInput<'_>,
    slide_size: &SlideSize,
    options: &BuildOptions,
) -> Result<PageReport> {
    let layout = input.layout;
    let (slide_cx, slide_cy) = slide_size.emu();
    let slide_width_pt = slide_cx as f64 / 12_700.0;
    let slide_height_pt = slide_cy as f64 / 12_700.0;

    if layout.size_pt.0 <= 0.0 || layout.size_pt.1 <= 0.0 {
        return Err(Error::BadPageSize(format!("{:?}", layout.size_pt)));
    }
    let scale = slide_width_pt / layout.size_pt.0;

    let mut report = PageReport::default();

    // ── 1. 文字框 ──────────────────────────────────────────────────────────
    for block in layout.text_blocks() {
        let Some(text_box) = text_box_for(block, scale, slide_width_pt, slide_height_pt, options)
        else {
            report.skipped_text += 1;
            continue;
        };
        deck.push_text_box(index, &text_box).map_err(Error::Ooxml)?;
        report.text_boxes += 1;
    }

    // ── 2. 前景圖片（裁切後置底）───────────────────────────────────────────
    let image_scale = input.image.width as f64 / layout.size_pt.0;
    for block in layout.foreground_blocks() {
        let expanded = expand_bbox(block.bbox, FOREGROUND_EXPAND_PX)
            .clamp(input.image.width as i64, input.image.height as i64);
        if expanded.is_empty() {
            continue;
        }
        let crop = input.image.crop(expanded);
        let png = crop.to_png().map_err(|e| Error::Image(e.to_string()))?;
        let rect = ppt_rect(block.bbox, scale);
        deck.insert_picture_at_bottom(index, png, "png", rect, "fg")
            .map_err(Error::Ooxml)?;
        report.pictures += 1;
    }

    // ── 3. 背景 ────────────────────────────────────────────────────────────
    let background = prepare_background(input, options)?;
    deck.set_background_picture(index, background, "png")
        .map_err(Error::Ooxml)?;

    let _ = image_scale;
    Ok(report)
}

/// 由區塊產生文字框；跳過空白與 NotebookLM 頁尾水印。
fn text_box_for(
    block: &Block,
    scale: f64,
    slide_width_pt: f64,
    slide_height_pt: f64,
    options: &BuildOptions,
) -> Option<TextBox> {
    if should_skip(block) {
        return None;
    }

    let bx1 = block.bbox.left as f64;
    let by1 = block.bbox.top as f64;
    let bx2 = block.bbox.right as f64;
    let by2 = block.bbox.bottom as f64;

    let height_pt = (by2 - by1) * scale;
    let line_count = block.lines.len().max(1);
    let font_size_pt = calculate_font_size(height_pt, line_count);

    let is_title = block.kind == BlockKind::ParagraphTitle;
    let (alignment, h_padding) = if is_title {
        (Align::Left, H_PADDING_TITLE_PT)
    } else {
        (Align::Center, H_PADDING_BODY_PT)
    };

    let left = bx1 * scale;
    let top = by1 * scale + TEXT_DELTA_Y_PT;
    let right = bx2 * scale;
    let bottom = by2 * scale + TEXT_DELTA_Y_PT;

    let rect = Rect::ltrb(
        left.max(0.0),
        (top - V_PADDING_PT).max(0.0),
        (right + h_padding + h_padding).min(slide_width_pt),
        (bottom + V_PADDING_PT).min(slide_height_pt),
    );
    if rect.width_pt() <= 0.0 || rect.height_pt() <= 0.0 {
        return None;
    }

    let mut text_box = TextBox::new(
        format!("Block_{}", block.kind.label()),
        block.text.clone(),
        rect,
    )
    .font(options.font_name.clone(), font_size_pt)
    .align(alignment);
    if options.text_autofit {
        text_box = text_box.autofit(AutoFit::Normal);
    }
    Some(text_box)
}

/// 對應 `should_skip_text_block`。
fn should_skip(block: &Block) -> bool {
    if !block.kind.is_text() {
        return true;
    }
    if block.text.trim().is_empty() {
        return true;
    }
    // 跳過 NotebookLM 的頁尾水印文字
    if block.kind == BlockKind::Footer && block.text.to_lowercase().contains("notebooklm") {
        return true;
    }
    false
}

/// 對應 `calculate_font_size`：以 bbox 高度推算字級。
fn calculate_font_size(height_pt: f64, line_count: usize) -> f64 {
    let font_size = if line_count > 1 {
        (height_pt / line_count as f64) * FONT_HEIGHT_RATIO
    } else {
        height_pt * FONT_HEIGHT_RATIO
    };
    font_size.max(MIN_FONT_SIZE_PT).round()
}

fn ppt_rect(bbox: BBox, scale: f64) -> Rect {
    Rect::ltrb(
        bbox.left as f64 * scale,
        bbox.top as f64 * scale,
        bbox.right as f64 * scale,
        bbox.bottom as f64 * scale,
    )
}

fn expand_bbox(bbox: BBox, expand: i64) -> BBox {
    BBox::new(
        bbox.left - expand,
        bbox.top - expand,
        bbox.right + expand,
        bbox.bottom + expand,
    )
}

/// 準備背景影像：選擇性擦除已處理的區域。
fn prepare_background(input: &PageInput<'_>, options: &BuildOptions) -> Result<Vec<u8>> {
    let layout = input.layout;
    let mut image = match options.background_resolution {
        BackgroundResolution::FullImage => input.image.clone(),
        BackgroundResolution::PageSize => resize_to(
            input.image,
            layout.size_pt.0.round().max(1.0) as u32,
            layout.size_pt.1.round().max(1.0) as u32,
        )?,
    };

    if options.erase_background_regions {
        let image_scale = image.width as f64 / layout.size_pt.0;
        for block in layout.erasable_blocks() {
            let bbox = expand_bbox(block.bbox.scale(image_scale), FOREGROUND_EXPAND_PX)
                .clamp(image.width as i64, image.height as i64);
            if bbox.is_empty() {
                continue;
            }
            let (_, fill_color) = edge_diversity(&image, bbox, ERASE_TOLERANCE);
            for y in bbox.top..bbox.bottom {
                for x in bbox.left..bbox.right {
                    image.set_pixel(x as u32, y as u32, fill_color);
                }
            }
        }
    }

    image.to_png().map_err(|e| Error::Image(e.to_string()))
}

/// 用 Lanczos3 縮放（對應 Pillow 的 `Image.LANCZOS`）。
fn resize_to(image: &RgbImage, width: u32, height: u32) -> Result<RgbImage> {
    if image.width == width && image.height == height {
        return Ok(image.clone());
    }
    let buffer = image::RgbImage::from_raw(image.width, image.height, image.data.clone())
        .ok_or_else(|| Error::Image("無法建立影像緩衝".into()))?;
    let resized = image::imageops::resize(
        &buffer,
        width,
        height,
        image::imageops::FilterType::Lanczos3,
    );
    RgbImage::from_raw(width, height, resized.into_raw()).map_err(|e| Error::Image(e.to_string()))
}

/// 便利函式：把一組頁面直接輸出成單一 PPTX 檔。
pub fn build_to_file(
    inputs: &[PageInput<'_>],
    out: impl AsRef<std::path::Path>,
    options: &BuildOptions,
) -> Result<BuildReport> {
    let (deck, report) = build_deck(inputs, options)?;
    deck.save(out).map_err(Error::Ooxml)?;
    Ok(report)
}

/// 讀取內嵌範本（供測試與診斷）。
pub fn blank_template() -> Result<Deck> {
    Deck::from_package(Package::from_bytes(BLANK_TEMPLATE).map_err(Error::Ooxml)?)
        .map_err(Error::Ooxml)
}

#[cfg(test)]
mod tests {
    use super::*;
    use n2p_layout::{Line, Span};

    fn page() -> LayoutPage {
        LayoutPage {
            size_pt: (960.0, 540.0),
            blocks: vec![
                Block {
                    kind: BlockKind::ParagraphTitle,
                    bbox: BBox::new(60, 30, 660, 80),
                    text: "標題".into(),
                    lines: vec![Line {
                        spans: vec![Span {
                            text: Some("標題".into()),
                            image_path: None,
                        }],
                        bbox: None,
                    }],
                },
                Block {
                    kind: BlockKind::Text,
                    bbox: BBox::new(60, 200, 660, 260),
                    text: "內文".into(),
                    lines: vec![Line {
                        spans: vec![Span {
                            text: Some("內文".into()),
                            image_path: None,
                        }],
                        bbox: None,
                    }],
                },
            ],
        }
    }

    #[test]
    fn font_size_follows_bbox_height() {
        assert_eq!(calculate_font_size(60.0, 1), 45.0);
        assert_eq!(calculate_font_size(60.0, 2), 23.0);
        assert_eq!(calculate_font_size(2.0, 1), MIN_FONT_SIZE_PT);
    }

    #[test]
    fn footer_watermark_is_skipped() {
        let mut block = Block::new(BlockKind::Footer, BBox::new(0, 0, 10, 10));
        block.text = "Made with NotebookLM".into();
        assert!(should_skip(&block));
        block.text = "第 1 頁".into();
        assert!(!should_skip(&block));
    }

    #[test]
    fn empty_text_is_skipped() {
        let block = Block::new(BlockKind::Text, BBox::new(0, 0, 10, 10));
        assert!(should_skip(&block));
    }

    #[test]
    fn blank_template_is_usable() {
        let deck = blank_template().unwrap();
        assert_eq!(deck.slide_count().unwrap(), 1);
        let names = n2p_ooxml::slide::shape_names(&deck.slide(0).unwrap()).unwrap();
        assert!(names.is_empty(), "範本不應含任何形狀，實際 {names:?}");
    }

    #[test]
    fn builds_one_slide_per_page_with_text_and_background() {
        let layout = page();
        let image = RgbImage::filled(1920, 1080, [240, 240, 240]);
        let inputs = vec![
            PageInput {
                layout: &layout,
                image: &image,
            },
            PageInput {
                layout: &layout,
                image: &image,
            },
        ];
        let (deck, report) = build_deck(&inputs, &BuildOptions::default()).unwrap();

        assert_eq!(deck.slide_count().unwrap(), 2);
        assert_eq!(report.pages.len(), 2);
        assert_eq!(report.pages[0].text_boxes, 2);
        assert_eq!(report.slide_size, SlideSize::Screen16x9);

        for index in 0..2 {
            let doc = deck.slide(index).unwrap();
            assert!(
                n2p_ooxml::slide::background(&doc).is_some(),
                "第 {index} 頁應有背景"
            );
            let names = n2p_ooxml::slide::shape_names(&doc).unwrap();
            assert_eq!(
                names,
                vec![
                    "Block_paragraph_title".to_string(),
                    "Block_text".to_string()
                ]
            );
            assert!(n2p_ooxml::slide::background_blip_rid(&doc).is_some());
        }
    }

    #[test]
    fn foreground_blocks_become_pictures_below_text() {
        let layout = LayoutPage {
            size_pt: (960.0, 540.0),
            blocks: vec![
                Block::new(BlockKind::Text, BBox::new(10, 10, 100, 40)),
                Block::new(BlockKind::Image, BBox::new(200, 200, 400, 400)),
            ],
        };
        let image = RgbImage::filled(1920, 1080, [120, 130, 140]);
        let (deck, report) = build_deck(
            &[PageInput {
                layout: &layout,
                image: &image,
            }],
            &BuildOptions::default(),
        )
        .unwrap();

        assert_eq!(report.pages[0].pictures, 1);
        let doc = deck.slide(0).unwrap();
        let kinds = n2p_ooxml::slide::shape_kinds(&doc).unwrap();
        assert_eq!(
            kinds.first(),
            Some(&"p:pic"),
            "圖片應在最底層，實際 {kinds:?}"
        );
    }

    #[test]
    fn empty_input_is_rejected() {
        assert!(matches!(
            build_deck(&[], &BuildOptions::default()),
            Err(Error::NoPages)
        ));
    }

    #[test]
    fn full_image_background_keeps_resolution() {
        let layout = page();
        let image = RgbImage::filled(1920, 1080, [1, 2, 3]);
        let options = BuildOptions {
            background_resolution: BackgroundResolution::FullImage,
            erase_background_regions: false,
            ..Default::default()
        };
        let (deck, _) = build_deck(
            &[PageInput {
                layout: &layout,
                image: &image,
            }],
            &options,
        )
        .unwrap();
        let media = deck
            .package()
            .part_names()
            .find(|p| p.starts_with("ppt/media/"))
            .expect("應有背景影像")
            .to_owned();
        let bytes = deck.package().part(&media).unwrap();
        let decoded = RgbImage::from_png_bytes(bytes).unwrap();
        assert_eq!((decoded.width, decoded.height), (1920, 1080));
    }
}
