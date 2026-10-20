//! `p:pic` 寫入器：插入圖片。
//!
//! 對照 `assets/probe-pptx/3_picture.pptx`。
//!
//! 注意 `<p:pic>` 與 `<p:sp>` 的子節點順序不同：
//! `pic` 是 `nvPicPr` → `blipFill` → `spPr`；`sp` 是 `nvSpPr` → `spPr` → `txBody`。

use crate::emu::Rect;
use crate::xml::Element;

/// 圖片形狀規格。
#[derive(Debug, Clone)]
pub struct Picture {
    pub name: String,
    /// 影像 part 的關聯 id（由 `Package::add_image_for` 取得）。
    pub embed_rid: String,
    pub rect: Rect,
    /// 是否關閉外框線（對應 `Line.FillType = none`）。
    pub no_line: bool,
}

impl Picture {
    pub fn new(name: impl Into<String>, embed_rid: impl Into<String>, rect: Rect) -> Self {
        Self {
            name: name.into(),
            embed_rid: embed_rid.into(),
            rect,
            no_line: true,
        }
    }

    /// 產生 `<p:pic>`。
    pub fn build(&self, shape_id: u32) -> Element {
        let (x, y) = self.rect.emu_offset();
        let (cx, cy) = self.rect.emu_extent();

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
        if self.no_line {
            sp_pr = sp_pr.child(Element::new("a:ln").child(Element::new("a:noFill")));
        }

        Element::new("p:pic")
            .child(
                Element::new("p:nvPicPr")
                    .child(
                        Element::new("p:cNvPr")
                            .attr("id", shape_id.to_string())
                            .attr("name", self.name.as_str()),
                    )
                    .child(Element::new("p:cNvPicPr"))
                    .child(Element::new("p:nvPr")),
            )
            .child(
                Element::new("p:blipFill")
                    .child(Element::new("a:blip").attr("r:embed", self.embed_rid.as_str()))
                    .child(Element::new("a:srcRect"))
                    .child(Element::new("a:stretch").child(Element::new("a:fillRect"))),
            )
            .child(sp_pr)
    }
}
