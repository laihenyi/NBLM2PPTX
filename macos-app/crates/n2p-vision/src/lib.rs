//! macOS Vision OCR → [`n2p_layout::LayoutPage`]。
//!
//! 這是取代 Windows 版「微軟電腦管家智慧圈選」的那一層：在裝置端辨識文字與邊界框，
//! 不上傳任何影像，也不需要外部服務。
//!
//! ## Vision 給我們什麼、不給什麼
//!
//! `VNRecognizeTextRequest` 回傳的是**文字觀測**（一段文字 + 正規化邊界框 + 信心度），
//! 不含語意標籤（標題／內文／圖片）。因此：
//!
//! * 文字一律建成 `BlockKind::Text`。需要 `paragraph_title`／`image_body` 這類語意時，
//!   請改用 MinerU JSON（`n2p-mineru`）。
//! * 不做圖片區塊偵測。圖片留在背景圖上不被擦除，視覺結果與 Windows 版一致
//!   （Windows 版是「文字變成文字框、其餘留在背景」）。
//!
//! ## 座標
//!
//! Vision 的 `boundingBox` 是**正規化**且**原點在左下、y 向上**；這裡轉成
//! 「pt、原點左上、y 向下」，與其他 provider 一致。

use n2p_core::{BBox, RgbImage};
use n2p_layout::{
    Block, BlockKind, Error as LayoutError, LayoutPage, LayoutProvider, Line,
    Result as LayoutResult, Span,
};

#[cfg(target_os = "macos")]
mod macos;

/// 一筆辨識結果。
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub text: String,
    /// 0.0 ~ 1.0。
    pub confidence: f32,
    /// 正規化邊界框：`(x, y, width, height)`，**原點左上、y 向下**。
    pub bbox_norm: (f64, f64, f64, f64),
}

/// 預設支援的 OCR 辨識語言（對應本專案支援的六種語言）。
pub const DEFAULT_LANGUAGES: &[&str] = &["zh-Hant", "zh-Hans", "ja-JP", "en-US", "es-ES", "fr-FR"];

/// 依 locale 標籤（例如 `ja-JP`、`zh_CN.UTF-8`、`fr-FR`）決定 Vision 辨識語言優先序。
///
/// 將使用者系統偏好的語言排在第一位、英文排在第二位，其餘預設語言依序補在後面，
/// 讓日文（平假名／片假名）、簡體中文、繁體中文與西歐語言都能在不手動指定 `--lang` 時正確辨識。
pub fn languages_for_locale(tag: &str) -> Vec<String> {
    let base = tag
        .trim()
        .split(['.', '@'])
        .next()
        .unwrap_or("")
        .replace('_', "-")
        .to_ascii_lowercase();
    let mut parts = base.split('-');
    let primary = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();

    let preferred = match primary {
        "ja" | "jp" => Some("ja-JP"),
        "zh" => {
            if rest.iter().any(|&s| s == "hans" || s == "cn" || s == "sg") {
                Some("zh-Hans")
            } else {
                Some("zh-Hant")
            }
        }
        "en" => Some("en-US"),
        "es" => Some("es-ES"),
        "fr" => Some("fr-FR"),
        _ => None,
    };

    let mut result = Vec::with_capacity(DEFAULT_LANGUAGES.len());
    if let Some(first) = preferred {
        result.push(first.to_owned());
        if first != "en-US" {
            result.push("en-US".to_owned());
        }
    }
    for &lang in DEFAULT_LANGUAGES {
        if !result.iter().any(|existing| existing == lang) {
            result.push(lang.to_owned());
        }
    }
    result
}

/// 依目前系統語言環境回傳預設的 Vision 辨識語言清單。
pub fn default_languages() -> Vec<String> {
    #[cfg(target_os = "macos")]
    for tag in macos::preferred_languages() {
        let langs = languages_for_locale(&tag);
        if langs.first().map(String::as_str) != DEFAULT_LANGUAGES.first().copied()
            || tag.to_ascii_lowercase().starts_with("zh")
        {
            return langs;
        }
    }
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            if !val.trim().is_empty() {
                return languages_for_locale(&val);
            }
        }
    }
    DEFAULT_LANGUAGES.iter().map(|&s| s.to_owned()).collect()
}

/// 辨識設定。
#[derive(Debug, Clone)]
pub struct VisionConfig {
    /// 語言優先序，例如 `["zh-Hant", "en-US"]`。
    pub languages: Vec<String>,
    /// `true` 用 `Accurate`（預設），`false` 用 `Fast`。
    pub accurate: bool,
    /// 是否開啟語言校正。
    pub language_correction: bool,
    /// 最小文字高度（相對於影像高度的比例）。太小的文字會被忽略。
    pub minimum_text_height: f32,
    /// 低於此信心度的觀測會被丟棄。
    pub min_confidence: f32,
    /// 是否把相鄰的行合併成一個區塊（讓文字框數量接近 MinerU 的區塊數）。
    pub group_lines: bool,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            languages: default_languages(),
            accurate: true,
            language_correction: true,
            minimum_text_height: 0.008,
            min_confidence: 0.3,
            group_lines: true,
        }
    }
}

/// 錯誤型別。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Vision 不支援此平台（僅 macOS）")]
    Unsupported,

    #[error("Vision 執行失敗：{0}")]
    Vision(String),

    #[error("編碼影像失敗：{0}")]
    Encode(String),

    #[error("頁面尺寸與影像數量不符：{pages} vs {images}")]
    PageCountMismatch { pages: usize, images: usize },
}

pub type Result<T> = std::result::Result<T, Error>;

/// 對一張影像做文字辨識。
pub fn recognize(image: &RgbImage, config: &VisionConfig) -> Result<Vec<Observation>> {
    let png = image.to_png().map_err(|e| Error::Encode(e.to_string()))?;
    recognize_png(&png, config)
}

/// 對 PNG bytes 做文字辨識。
pub fn recognize_png(png: &[u8], config: &VisionConfig) -> Result<Vec<Observation>> {
    #[cfg(target_os = "macos")]
    {
        macos::recognize_png(png, config)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (png, config);
        Err(Error::Unsupported)
    }
}

/// 一份由 Vision 產生的版面文件。
#[derive(Debug, Clone, Default)]
pub struct VisionDocument {
    pages: Vec<LayoutPage>,
    /// 每頁被丟棄的低信心度觀測數，方便診斷「為什麼這頁字很少」。
    dropped: Vec<usize>,
}

impl VisionDocument {
    /// 對每張頁面影像做 OCR，產生版面。
    ///
    /// `page_sizes_pt` 是每頁的實際尺寸（pt）；若為空則以影像像素數當作 pt（1:1）。
    pub fn from_images(
        images: &[RgbImage],
        page_sizes_pt: &[(f64, f64)],
        config: &VisionConfig,
    ) -> Result<Self> {
        if !page_sizes_pt.is_empty() && page_sizes_pt.len() != images.len() {
            return Err(Error::PageCountMismatch {
                pages: page_sizes_pt.len(),
                images: images.len(),
            });
        }

        let mut pages = Vec::with_capacity(images.len());
        let mut dropped = Vec::with_capacity(images.len());

        for (index, image) in images.iter().enumerate() {
            let size_pt = page_sizes_pt
                .get(index)
                .copied()
                .unwrap_or((image.width as f64, image.height as f64));
            let observations = recognize(image, config)?;
            let total = observations.len();
            let kept: Vec<Observation> = observations
                .into_iter()
                .filter(|o| o.confidence >= config.min_confidence && !o.text.trim().is_empty())
                .collect();
            dropped.push(total - kept.len());

            let blocks = if config.group_lines {
                group_observations(&kept, size_pt)
            } else {
                kept.iter()
                    .map(|o| block_from_observation(o, size_pt))
                    .collect()
            };
            pages.push(LayoutPage { size_pt, blocks });
        }

        Ok(Self { pages, dropped })
    }

    pub fn dropped(&self) -> &[usize] {
        &self.dropped
    }
}

impl LayoutProvider for VisionDocument {
    fn name(&self) -> &str {
        "vision"
    }

    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn page(&self, index: usize) -> LayoutResult<LayoutPage> {
        self.pages
            .get(index)
            .cloned()
            .ok_or(LayoutError::PageOutOfRange {
                index,
                count: self.pages.len(),
            })
    }
}

/// 正規化座標 → 像素／點座標。
///
/// 先把乘完的值吸附到 1e-6 精度再取整，避免 `0.16 + 0.05 = 0.21000000000000002`
/// 這種浮點誤差讓 `ceil` 多跳一格。
fn norm_to_px(value: f64, scale: f64, round_up: bool) -> i64 {
    let scaled = value * scale;
    let snapped = (scaled * 1e6).round() / 1e6;
    if round_up {
        snapped.ceil() as i64
    } else {
        snapped.floor() as i64
    }
}

fn block_from_observation(observation: &Observation, size_pt: (f64, f64)) -> Block {
    let (x, y, w, h) = observation.bbox_norm;
    let bbox = BBox::new(
        norm_to_px(x, size_pt.0, false),
        norm_to_px(y, size_pt.1, false),
        norm_to_px(x + w, size_pt.0, true),
        norm_to_px(y + h, size_pt.1, true),
    );
    let mut block = Block::new(BlockKind::Text, bbox);
    block.text = observation.text.clone();
    block.lines = vec![Line {
        spans: vec![Span {
            text: Some(observation.text.clone()),
            image_path: None,
        }],
        bbox: Some(bbox),
    }];
    block
}

/// 把相鄰的觀測合併成區塊。
///
/// 規則（刻意保守，寧可少併也不要錯併）：
///
/// 1. 依垂直位置排序（上到下），同高則由左到右。
/// 2. 只有同時滿足以下三點才併入同一區塊：
///    * 垂直間距 ≤ `0.5 × 較小那行的高度`
///    * 水平重疊 > 30%
///    * **兩行高度比 ≤ 1.5**（字級差太多就不是同一段）
/// 3. 否則開新區塊。
///
/// 第三條是實測補上的：`CH9` 第 1 頁的大標題行高 0.185、下方小字行高 0.062，
/// 原本只用「較大行高 × 0.5」判斷間距，結果小字被吸進標題區塊，整段跟著用
/// 88 pt 渲染 —— 字級看起來完全失控。
pub fn group_observations(observations: &[Observation], size_pt: (f64, f64)) -> Vec<Block> {
    if observations.is_empty() {
        return Vec::new();
    }

    let mut sorted: Vec<&Observation> = observations.iter().collect();
    sorted.sort_by(|a, b| {
        let ay = a.bbox_norm.1;
        let by = b.bbox_norm.1;
        ay.partial_cmp(&by)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.bbox_norm
                    .0
                    .partial_cmp(&b.bbox_norm.0)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    let mut groups: Vec<Vec<&Observation>> = Vec::new();
    for observation in sorted {
        let merged = groups.last_mut().and_then(|group| {
            let last = group.last()?;
            let last_bottom = last.bbox_norm.1 + last.bbox_norm.3;
            let next_top = observation.bbox_norm.1;
            let small = last.bbox_norm.3.min(observation.bbox_norm.3);
            let large = last.bbox_norm.3.max(observation.bbox_norm.3);
            if small <= 0.0 {
                return None;
            }
            let vertical_gap = next_top - last_bottom;
            let overlap = overlap_ratio(last.bbox_norm, observation.bbox_norm);
            let same_scale = large / small <= 1.5;
            (same_scale && vertical_gap <= small * 0.5 && overlap > 0.3).then_some(group)
        });
        match merged {
            Some(group) => group.push(observation),
            None => groups.push(vec![observation]),
        }
    }

    groups
        .into_iter()
        .map(|group| {
            let mut union = group[0].bbox_norm;
            for observation in &group[1..] {
                union = union_of(union, observation.bbox_norm);
            }
            let bbox = BBox::new(
                norm_to_px(union.0, size_pt.0, false),
                norm_to_px(union.1, size_pt.1, false),
                norm_to_px(union.0 + union.2, size_pt.0, true),
                norm_to_px(union.1 + union.3, size_pt.1, true),
            );
            let text = group
                .iter()
                .map(|o| o.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let mut block = Block::new(BlockKind::Text, bbox);
            block.text = text.clone();
            block.lines = group
                .iter()
                .map(|o| {
                    let line_bbox = BBox::new(
                        norm_to_px(o.bbox_norm.0, size_pt.0, false),
                        norm_to_px(o.bbox_norm.1, size_pt.1, false),
                        norm_to_px(o.bbox_norm.0 + o.bbox_norm.2, size_pt.0, true),
                        norm_to_px(o.bbox_norm.1 + o.bbox_norm.3, size_pt.1, true),
                    );
                    Line {
                        spans: vec![Span {
                            text: Some(o.text.clone()),
                            image_path: None,
                        }],
                        bbox: Some(line_bbox),
                    }
                })
                .collect();
            block
        })
        .collect()
}

fn overlap_ratio(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> f64 {
    let left = a.0.max(b.0);
    let right = (a.0 + a.2).min(b.0 + b.2);
    let intersection = (right - left).max(0.0);
    let smaller = a.2.min(b.2);
    if smaller <= 0.0 {
        0.0
    } else {
        intersection / smaller
    }
}

fn union_of(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let left = a.0.min(b.0);
    let top = a.1.min(b.1);
    let right = (a.0 + a.2).max(b.0 + b.2);
    let bottom = (a.1 + a.3).max(b.1 + b.3);
    (left, top, right - left, bottom - top)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(text: &str, x: f64, y: f64, w: f64, h: f64) -> Observation {
        Observation {
            text: text.into(),
            confidence: 0.9,
            bbox_norm: (x, y, w, h),
        }
    }

    #[test]
    fn groups_adjacent_lines_with_overlap() {
        let observations = vec![
            obs("第一行", 0.1, 0.10, 0.4, 0.05),
            obs("第二行", 0.1, 0.16, 0.4, 0.05),
            obs("遠處", 0.1, 0.80, 0.4, 0.05),
        ];
        let blocks = group_observations(&observations, (1000.0, 500.0));
        assert_eq!(blocks.len(), 2, "前兩行應合併，第三行獨立");
        assert_eq!(blocks[0].text, "第一行\n第二行");
        assert_eq!(blocks[0].lines.len(), 2);
        assert_eq!(blocks[1].text, "遠處");
    }

    #[test]
    fn does_not_group_lines_with_very_different_sizes() {
        // CH9 第 1 頁的真實幾何：大標題第二行 + 下方小字
        let observations = vec![
            obs("好好愛他們！", 0.238, 0.406, 0.508, 0.185),
            obs("創造共享的溫暖", 0.552, 0.649, 0.217, 0.062),
        ];
        let blocks = group_observations(&observations, (1376.0, 768.0));
        assert_eq!(blocks.len(), 2, "字級差 3 倍的行不該併成同一區塊");
    }

    #[test]
    fn still_groups_lines_of_similar_size() {
        let observations = vec![
            obs("提升學生正向幸福", 0.217, 0.649, 0.244, 0.060),
            obs("感的課堂領導力", 0.229, 0.716, 0.217, 0.067),
        ];
        let blocks = group_observations(&observations, (1376.0, 768.0));
        assert_eq!(blocks.len(), 1, "同級的小字應該併成一個區塊");
        assert_eq!(blocks[0].lines.len(), 2);
    }

    #[test]
    fn does_not_group_unrelated_columns() {
        let observations = vec![
            obs("左欄", 0.05, 0.10, 0.2, 0.05),
            obs("右欄", 0.70, 0.11, 0.2, 0.05),
        ];
        let blocks = group_observations(&observations, (1000.0, 500.0));
        assert_eq!(blocks.len(), 2, "水平不重疊不應合併");
    }

    #[test]
    fn converts_normalized_boxes_to_points() {
        let observations = vec![obs("x", 0.25, 0.5, 0.5, 0.1)];
        let blocks = group_observations(&observations, (1000.0, 400.0));
        assert_eq!(blocks[0].bbox, BBox::new(250, 200, 750, 240));
    }

    #[test]
    fn union_grows_with_each_line() {
        let observations = vec![
            obs("a", 0.10, 0.10, 0.30, 0.05),
            obs("b", 0.20, 0.16, 0.50, 0.05),
        ];
        let blocks = group_observations(&observations, (100.0, 100.0));
        assert_eq!(blocks.len(), 1);
        // union: x 0.10..0.70, y 0.10..0.21
        assert_eq!(blocks[0].bbox, BBox::new(10, 10, 70, 21));
    }

    #[test]
    fn empty_input_yields_no_blocks() {
        assert!(group_observations(&[], (100.0, 100.0)).is_empty());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_macos_reports_unsupported() {
        let image = RgbImage::filled(10, 10, [0, 0, 0]);
        assert!(matches!(
            recognize(&image, &VisionConfig::default()),
            Err(Error::Unsupported)
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recognizes_text_in_a_generated_image() {
        // 產生一張有黑色方塊與文字的圖，確認 Vision 至少能跑完並回傳結構。
        // 這裡只驗證「管線可用」，文字內容正確性由 CLI 的端到端測試負責。
        let image = RgbImage::filled(400, 120, [255, 255, 255]);
        let observations = recognize(&image, &VisionConfig::default()).unwrap();
        // 空白圖不應誤判出大量文字
        assert!(
            observations.len() < 5,
            "空白圖竟辨識出 {} 筆",
            observations.len()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn vision_document_exposes_pages() {
        let image = RgbImage::filled(200, 100, [255, 255, 255]);
        let doc =
            VisionDocument::from_images(&[image], &[(200.0, 100.0)], &VisionConfig::default())
                .unwrap();
        assert_eq!(doc.page_count(), 1);
        assert_eq!(doc.page(0).unwrap().size_pt, (200.0, 100.0));
        assert_eq!(doc.dropped().len(), 1);
    }
}
