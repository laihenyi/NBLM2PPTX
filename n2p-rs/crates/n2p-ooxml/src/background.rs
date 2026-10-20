//! `p:bg` 寫入器：投影片背景填色。
//!
//! 對照 `assets/probe-pptx/2_background.pptx`。
//!
//! 對應 Spire 的：
//! ```text
//! slide.SlideBackground.Type = BackgroundType.Custom
//! slide.SlideBackground.Fill.FillType = FillFormatType.Picture
//! slide.SlideBackground.Fill.PictureFill.Picture.EmbedImage = <image>
//! slide.SlideBackground.Fill.PictureFill.FillType = PictureFillType.Stretch
//! ```

use crate::xml::Element;

/// 背景填滿方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Background {
    /// 圖片填滿（Stretch）。
    Picture {
        /// `Package::add_image_for` 取得的關聯 id。
        embed_rid: String,
        /// 是否保留 Spire 的 `dpi="0"` 屬性（PowerPoint 也接受省略）。
        dpi_zero: bool,
    },
    /// 純色填滿。
    Solid { rgb: [u8; 3] },
}

impl Background {
    pub fn picture(embed_rid: impl Into<String>) -> Self {
        Self::Picture {
            embed_rid: embed_rid.into(),
            dpi_zero: true,
        }
    }

    pub fn solid(rgb: [u8; 3]) -> Self {
        Self::Solid { rgb }
    }

    /// 產生 `<p:bg>`。
    pub fn build(&self) -> Element {
        match self {
            Self::Picture {
                embed_rid,
                dpi_zero,
            } => {
                let mut blip_fill = Element::new("a:blipFill");
                if *dpi_zero {
                    blip_fill.set_attr("dpi", "0");
                }
                blip_fill = blip_fill
                    .child(Element::new("a:blip").attr("r:embed", embed_rid.as_str()))
                    .child(Element::new("a:srcRect"))
                    .child(Element::new("a:stretch").child(Element::new("a:fillRect")));

                Element::new("p:bg").child(Element::new("p:bgPr").child(blip_fill))
            }
            Self::Solid { rgb } => Element::new("p:bg").child(
                Element::new("p:bgPr").child(
                    Element::new("a:solidFill").child(
                        Element::new("a:srgbClr")
                            .attr("val", format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])),
                    ),
                ),
            ),
        }
    }
}
