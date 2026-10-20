//! `p:sp` 寫入器：矩形文字框。
//!
//! 節點結構與屬性逐項對照 `assets/probe-pptx/1_text.pptx` 的
//! `ppt/slides/slide1.xml`（Spire.Presentation 11.8.1 實測輸出），
//! 由 `tests/golden.rs` 做結構比對。

use crate::emu::{Rect, centipoint_from_pt};
use crate::xml::Element;

/// 段落水平對齊，對應 `TextAlignmentType`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
    Justify,
    Distributed,
}

impl Align {
    pub fn token(self) -> &'static str {
        match self {
            Self::Left => "l",
            Self::Center => "ctr",
            Self::Right => "r",
            Self::Justify => "just",
            Self::Distributed => "dist",
        }
    }
}

/// 文字框垂直錨點。Spire 建立新形狀時預設居中（`ctr`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Top,
    Middle,
    Bottom,
}

impl Anchor {
    pub fn token(self) -> &'static str {
        match self {
            Self::Top => "t",
            Self::Middle => "ctr",
            Self::Bottom => "b",
        }
    }
}

/// 文字自動縮放。預設 `None` 是**刻意還原 Spire 的既有行為**。
///
/// 實測 Spire 11.8.1 對 `TextFrame.FitTextToShape = True` 不寫任何 autofit 元素，
/// 因此目前產出的 PPTX 完全依賴 PowerPoint 預設排版 —— 這也是 MinerU 重排版後
/// 文字偶爾溢出的原因。改成 `AutoFit::Normal` 可修好這件事，但屬於行為變更。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoFit {
    /// 不寫 autofit（Spire 等價）。
    None,
    /// `<a:normAutofit/>`：縮放文字與行距以塞入框內。
    Normal,
    /// `<a:spAutoFit/>`：反過來把框縮到文字大小。
    Shape,
}

/// 多行文字的表示方式。
///
/// 實測 Spire 11.8.1 對 `TextFrame.Text = "a\\nb"` 的處理是：**單一段落，換行字元
/// 原樣寫進 `<a:t>`**：
///
/// ```xml
/// <a:t>line one
/// line two</a:t>
/// ```
///
/// 這是 PowerPoint 與 LibreOffice 都容忍、但 DrawingML 規格上不算正規的寫法
/// （換行的正式表示法是 `<a:br/>`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewlineMode {
    /// 用 `<a:br/>` 分段。規格正確、視覺與 `Literal` 相同。**預設。**
    #[default]
    LineBreak,
    /// 換行字元原樣寫進 `a:t`。與 Spire 逐位元等價，便於回歸比對。
    Literal,
    /// 每個換行變成獨立段落 `a:p`。段落間距可能與前兩者不同。
    Paragraph,
}

/// 一個矩形文字框的完整規格。
#[derive(Debug, Clone)]
pub struct TextBox {
    pub name: String,
    /// 可含 `\n`；每個換行會變成獨立段落（`a:p`）。
    pub text: String,
    pub rect: Rect,
    pub font_family: String,
    pub font_size_pt: f64,
    pub color_rgb: [u8; 3],
    pub align: Align,
    pub anchor: Anchor,
    pub lang: String,
    /// 是否把四邊內距設為 0（對應 `MarginLeft/Right/Top/Bottom = 0`）。
    pub zero_insets: bool,
    pub autofit: AutoFit,
    /// 是否輸出 `<p:style>` 子樹。Spire 會輸出，可省略而不影響視覺。
    pub emit_style: bool,
    /// 多行文字如何表示。
    pub newline: NewlineMode,
}

impl Default for TextBox {
    fn default() -> Self {
        Self {
            name: "Block_text".to_owned(),
            text: String::new(),
            rect: Rect::ltrb(60.0, 40.0, 660.0, 120.0),
            font_family: "Calibri".to_owned(),
            font_size_pt: 24.0,
            color_rgb: [0, 0, 0],
            align: Align::Center,
            anchor: Anchor::Middle,
            lang: "en-US".to_owned(),
            zero_insets: true,
            autofit: AutoFit::None,
            emit_style: true,
            newline: NewlineMode::default(),
        }
    }
}

impl TextBox {
    pub fn new(name: impl Into<String>, text: impl Into<String>, rect: Rect) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
            rect,
            ..Default::default()
        }
    }

    pub fn font(mut self, family: impl Into<String>, size_pt: f64) -> Self {
        self.font_family = family.into();
        self.font_size_pt = size_pt;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn color(mut self, rgb: [u8; 3]) -> Self {
        self.color_rgb = rgb;
        self
    }

    pub fn autofit(mut self, autofit: AutoFit) -> Self {
        self.autofit = autofit;
        self
    }

    pub fn newline(mut self, mode: NewlineMode) -> Self {
        self.newline = mode;
        self
    }

    /// 產生 `<p:sp>`。
    pub fn build(&self, shape_id: u32) -> Element {
        let (x, y) = self.rect.emu_offset();
        let (cx, cy) = self.rect.emu_extent();

        let mut body_pr = Element::new("a:bodyPr");
        if self.zero_insets {
            body_pr.set_attr("lIns", "0");
            body_pr.set_attr("tIns", "0");
            body_pr.set_attr("rIns", "0");
            body_pr.set_attr("bIns", "0");
        }
        body_pr.set_attr("rtlCol", "0");
        body_pr.set_attr("anchor", self.anchor.token());
        match self.autofit {
            AutoFit::None => {}
            AutoFit::Normal => body_pr = body_pr.child(Element::new("a:normAutofit")),
            AutoFit::Shape => body_pr = body_pr.child(Element::new("a:spAutoFit")),
        }

        let paragraphs = match self.newline {
            NewlineMode::Literal => {
                vec![self.build_paragraph_from_runs(vec![Patch::Text(&self.text)])]
            }
            NewlineMode::LineBreak => {
                let mut runs = Vec::new();
                for (index, line) in self.text.split('\n').enumerate() {
                    if index > 0 {
                        runs.push(Patch::Break);
                    }
                    runs.push(Patch::Text(line));
                }
                vec![self.build_paragraph_from_runs(runs)]
            }
            NewlineMode::Paragraph => self
                .text
                .split('\n')
                .map(|line| self.build_paragraph_from_runs(vec![Patch::Text(line)]))
                .collect(),
        };

        let tx_body = Element::new("p:txBody")
            .child(body_pr)
            .child(Element::new("a:lstStyle"))
            .children(paragraphs);

        let mut sp_pr = Element::new("p:spPr").child(
            Element::new("a:xfrm")
                .child(
                    Element::new("a:off")
                        .attr("x", x.to_string())
                        .attr("y", y.to_string()),
                )
                .child(
                    Element::new("a:ext")
                        .attr("cx", cx.to_string())
                        .attr("cy", cy.to_string()),
                ),
        );
        sp_pr = sp_pr.child(Element::new("a:prstGeom").attr("prst", "rect"));
        // Fill.FillType = none
        sp_pr = sp_pr.child(Element::new("a:noFill"));
        // Line.FillType = none
        sp_pr = sp_pr.child(Element::new("a:ln").child(Element::new("a:noFill")));

        let mut sp = Element::new("p:sp")
            .child(
                Element::new("p:nvSpPr")
                    .child(
                        Element::new("p:cNvPr")
                            .attr("id", shape_id.to_string())
                            .attr("name", self.name.as_str()),
                    )
                    .child(Element::new("p:cNvSpPr"))
                    .child(Element::new("p:nvPr")),
            )
            .child(sp_pr);

        if self.emit_style {
            sp = sp.child(default_shape_style());
        }
        sp.child(tx_body)
    }

    fn build_paragraph_from_runs(&self, runs: Vec<Patch<'_>>) -> Element {
        let mut paragraph =
            Element::new("a:p").child(Element::new("a:pPr").attr("algn", self.align.token()));
        for run in runs {
            match run {
                Patch::Text(text) => paragraph = paragraph.child(self.build_run(text)),
                Patch::Break => paragraph = paragraph.child(Element::new("a:br")),
            }
        }
        paragraph
    }

    fn build_run(&self, text: &str) -> Element {
        let r_pr = Element::new("a:rPr")
            .attr("lang", self.lang.as_str())
            .attr("sz", centipoint_from_pt(self.font_size_pt).to_string())
            .attr("dirty", "1")
            .child(
                Element::new("a:solidFill").child(Element::new("a:srgbClr").attr(
                    "val",
                    format!(
                        "{:02X}{:02X}{:02X}",
                        self.color_rgb[0], self.color_rgb[1], self.color_rgb[2]
                    ),
                )),
            )
            .child(Element::new("a:latin").attr("typeface", self.font_family.as_str()));

        Element::new("a:r")
            .child(r_pr)
            .child(Element::text("a:t", text))
    }
}

/// 段落內的一個片段。
#[derive(Debug, Clone, Copy)]
enum Patch<'a> {
    Text(&'a str),
    Break,
}

/// Spire 對新建形狀附加的預設主題參照。
///
/// 視覺上被 `spPr` 的 `a:noFill` / `a:ln/a:noFill` 覆蓋，省略不影響呈現，
/// 但保留可讓輸出與既有版本更接近。
fn default_shape_style() -> Element {
    Element::new("p:style")
        .child(
            Element::new("a:lnRef").attr("idx", "2").child(
                Element::new("a:schemeClr")
                    .attr("val", "accent1")
                    .child(Element::new("a:shade").attr("val", "50000")),
            ),
        )
        .child(
            Element::new("a:fillRef")
                .attr("idx", "1")
                .child(Element::new("a:schemeClr").attr("val", "accent1")),
        )
        .child(
            Element::new("a:effectRef")
                .attr("idx", "0")
                .child(Element::new("a:schemeClr").attr("val", "accent1")),
        )
        .child(
            Element::new("a:fontRef")
                .attr("idx", "minor")
                .child(Element::new("a:schemeClr").attr("val", "lt1")),
        )
}
