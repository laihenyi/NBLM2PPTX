//! 中立的版面模型。
//!
//! 這一層刻意與來源解耦：MinerU JSON、PaddleOCR JSON、macOS Vision OCR 都先轉成
//! [`LayoutPage`]，再由 `n2p-build` 產生 PPTX。這樣「換 OCR 引擎」不會動到產生器。
//!
//! 座標一律是**原始頁面點數（pt）**，原點在左上角，單位為點而非像素 ——
//! 與 MinerU 的 `bbox` 一致。

use n2p_core::BBox;

/// 區塊種類。
///
/// `title`／`text` 等字串與 MinerU 的 label 對齊，因此可以直接互轉。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BlockKind {
    Title,
    ParagraphTitle,
    Text,
    Header,
    Footer,
    Reference,
    Algorithm,
    Image,
    Table,
    Chart,
    /// 其他來源自訂的 label，原樣保留。
    Other(String),
}

impl BlockKind {
    /// 由 MinerU／Paddle 的 label 建立。
    pub fn parse(label: &str) -> Self {
        let normalized = label.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "title" => Self::Title,
            "paragraph_title" => Self::ParagraphTitle,
            "text" => Self::Text,
            "header" => Self::Header,
            "footer" => Self::Footer,
            "reference" => Self::Reference,
            "algorithm" => Self::Algorithm,
            // MinerU 用 `image_body`／`table_body`，Paddle 用 `image`／`table`
            "image" | "image_body" => Self::Image,
            "table" | "table_body" => Self::Table,
            "chart" => Self::Chart,
            other => Self::Other(other.to_owned()),
        }
    }

    /// 原始 label。
    pub fn label(&self) -> &str {
        match self {
            Self::Title => "title",
            Self::ParagraphTitle => "paragraph_title",
            Self::Text => "text",
            Self::Header => "header",
            Self::Footer => "footer",
            Self::Reference => "reference",
            Self::Algorithm => "algorithm",
            Self::Image => "image",
            Self::Table => "table",
            Self::Chart => "chart",
            Self::Other(other) => other,
        }
    }

    /// 是否會變成文字框（對應 `should_skip_text_block` 的白名單）。
    pub fn is_text(&self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Title
                | Self::Header
                | Self::Footer
                | Self::Reference
                | Self::ParagraphTitle
                | Self::Algorithm
        )
    }

    /// 是否要被裁切成獨立圖片（`extract_foreground_element` 的條件）。
    pub fn is_foreground_image(&self) -> bool {
        matches!(self, Self::Image | Self::Table | Self::Chart)
    }

    /// 是否要在背景圖上擦除（`erasable_labels`）。
    pub fn is_erasable(&self) -> bool {
        self.is_text() || self.is_foreground_image()
    }
}

/// 一行文字中的片段。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Span {
    pub text: Option<String>,
    /// 若這個 span 代表一張圖，這裡是來源路徑或 URL。
    pub image_path: Option<String>,
}

/// 一行。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
    /// 這一行的聯集邊界框（pt）。
    pub bbox: Option<BBox>,
}

impl Line {
    /// 串接該行所有文字。
    pub fn text(&self) -> String {
        self.spans
            .iter()
            .filter_map(|s| s.text.as_deref())
            .collect::<Vec<_>>()
            .join("")
    }
}

/// 一個版面區塊。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    /// 邊界框（pt，左上原點）。
    pub bbox: BBox,
    /// 區塊文字（多行以 `\n` 連接）。
    pub text: String,
    pub lines: Vec<Line>,
    /// 順時針旋轉角度（千分之一度，例如 `-28_200` 代表 `-28.2°`）。
    pub rotation_mdeg: i32,
}

impl Block {
    pub fn new(kind: BlockKind, bbox: BBox) -> Self {
        Self {
            kind,
            bbox,
            text: String::new(),
            lines: Vec::new(),
            rotation_mdeg: 0,
        }
    }

    /// 順時針旋轉角度（度）。
    pub fn rotation_deg(&self) -> f64 {
        self.rotation_mdeg as f64 / 1000.0
    }

    /// 若這個區塊含圖片 span，回傳第一個路徑。
    pub fn image_path(&self) -> Option<&str> {
        self.lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find_map(|s| s.image_path.as_deref())
    }
}

/// 一頁的版面。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutPage {
    /// 頁面尺寸（pt）。
    pub size_pt: (f64, f64),
    pub blocks: Vec<Block>,
}

impl LayoutPage {
    pub fn text_blocks(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter().filter(|b| b.kind.is_text())
    }

    pub fn foreground_blocks(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter().filter(|b| b.kind.is_foreground_image())
    }

    pub fn erasable_blocks(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter().filter(|b| b.kind.is_erasable())
    }

    /// 依比例縮放所有座標（例如把 pt 換成像素）。
    pub fn scaled(&self, factor: f64) -> LayoutPage {
        LayoutPage {
            size_pt: (self.size_pt.0 * factor, self.size_pt.1 * factor),
            blocks: self
                .blocks
                .iter()
                .map(|b| Block {
                    kind: b.kind.clone(),
                    bbox: b.bbox.scale(factor),
                    text: b.text.clone(),
                    lines: b
                        .lines
                        .iter()
                        .map(|l| Line {
                            spans: l.spans.clone(),
                            bbox: l.bbox.map(|bb| bb.scale(factor)),
                        })
                        .collect(),
                    rotation_mdeg: b.rotation_mdeg,
                })
                .collect(),
        }
    }

    /// 水平平移所有座標並指定新頁寬（16:9 寬屏化時補白／裁切用）。
    ///
    /// 擴寬時 `dx` 為正（右移），裁切時為負（左移）；`new_width_pt` 是寬屏化後的頁寬。
    pub fn translated_x(&self, dx: f64, new_width_pt: f64) -> LayoutPage {
        let shift = |b: BBox| {
            BBox::new(
                b.left + dx.round() as i64,
                b.top,
                b.right + dx.round() as i64,
                b.bottom,
            )
        };
        LayoutPage {
            size_pt: (new_width_pt, self.size_pt.1),
            blocks: self
                .blocks
                .iter()
                .map(|b| Block {
                    kind: b.kind.clone(),
                    bbox: shift(b.bbox),
                    text: b.text.clone(),
                    lines: b
                        .lines
                        .iter()
                        .map(|l| Line {
                            spans: l.spans.clone(),
                            bbox: l.bbox.map(shift),
                        })
                        .collect(),
                    rotation_mdeg: b.rotation_mdeg,
                })
                .collect(),
        }
    }
}

/// 錯誤型別。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("版面資料格式不正確：{0}")]
    Malformed(String),

    #[error("頁索引 {index} 超出範圍（共 {count} 頁）")]
    PageOutOfRange { index: usize, count: usize },
}

pub type Result<T> = std::result::Result<T, Error>;

/// 版面來源。
pub trait LayoutProvider {
    /// 顯示用名稱（例如 `mineru`、`vision`）。
    fn name(&self) -> &str;

    fn page_count(&self) -> usize;

    fn page(&self, index: usize) -> Result<LayoutPage>;

    /// 方便走訪所有頁。
    fn pages(&self) -> Result<Vec<LayoutPage>> {
        (0..self.page_count()).map(|i| self.page(i)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_parsing_covers_mineru_and_paddle_labels() {
        assert_eq!(BlockKind::parse("image_body"), BlockKind::Image);
        assert_eq!(BlockKind::parse("image"), BlockKind::Image);
        assert_eq!(BlockKind::parse("table_body"), BlockKind::Table);
        assert_eq!(BlockKind::parse("Title"), BlockKind::Title);
        assert_eq!(BlockKind::parse("weird"), BlockKind::Other("weird".into()));
        assert_eq!(BlockKind::parse("weird").label(), "weird");
    }

    #[test]
    fn text_and_erasable_classification() {
        assert!(BlockKind::Text.is_text());
        assert!(BlockKind::ParagraphTitle.is_text());
        assert!(!BlockKind::Image.is_text());
        assert!(BlockKind::Image.is_erasable());
        assert!(!BlockKind::Other("x".into()).is_erasable());
    }

    #[test]
    fn line_text_joins_spans() {
        let line = Line {
            spans: vec![
                Span {
                    text: Some("Hello ".into()),
                    image_path: None,
                },
                Span {
                    text: Some("world".into()),
                    image_path: None,
                },
            ],
            bbox: None,
        };
        assert_eq!(line.text(), "Hello world");
    }

    #[test]
    fn scaling_and_shifting() {
        let page = LayoutPage {
            size_pt: (100.0, 50.0),
            blocks: vec![Block::new(BlockKind::Text, BBox::new(10, 10, 20, 20))],
        };
        let doubled = page.scaled(2.0);
        assert_eq!(doubled.size_pt, (200.0, 100.0));
        assert_eq!(doubled.blocks[0].bbox, BBox::new(20, 20, 40, 40));

        let shifted = page.translated_x(5.0, 130.0);
        assert_eq!(shifted.blocks[0].bbox, BBox::new(15, 10, 25, 20));
        assert_eq!(shifted.size_pt, (130.0, 50.0));
    }
}
