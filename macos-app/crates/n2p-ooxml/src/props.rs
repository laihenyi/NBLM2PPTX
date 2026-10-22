//! `docProps/app.xml`：擴充屬性。
//!
//! 這裡刻意修正 Spire.Presentation 11.8.1 的兩個 bug：
//!
//! 1. `PresentationFormat` 對 16:9 畫布仍寫成「全屏显示(4:3)」，會誤導依此判斷版面的工具。
//! 2. `Slides` 在多頁合併檔裡仍寫成 `1`，沒有跟著更新。
//!
//! 保留 Spire 的其餘欄位（`HeadingPairs` 等）以求相容，只覆寫該修的兩個值。

use crate::emu::SlideSize;
use crate::error::Result;
use crate::opc::Package;
use crate::pres::{self, PRESENTATION_PART};
use crate::xml::{Document, Element, Node};

pub const APP_PART: &str = "docProps/app.xml";

const NS_EXTENDED: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties";
const NS_VT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes";

/// 依畫布尺寸回傳對應的 `PresentationFormat` 字串。
///
/// 用英文標準名稱而非在地化字串，避免把介面語言寫進檔案屬性。
pub fn presentation_format(size: SlideSize) -> &'static str {
    match size {
        SlideSize::Screen16x9 => "Widescreen",
        SlideSize::Screen16x10 => "On-screen Show (16:10)",
        SlideSize::Screen4x3 => "On-screen Show (4:3)",
        SlideSize::Custom { .. } => "Custom",
    }
}

/// 由 `sldSz` 反推 `SlideSize`（無法對應到具名尺寸時回 `Custom`）。
pub fn slide_size_from_emu(cx: i64, cy: i64) -> SlideSize {
    for size in [
        SlideSize::Screen16x9,
        SlideSize::Screen16x10,
        SlideSize::Screen4x3,
    ] {
        if size.emu() == (cx, cy) {
            return size;
        }
    }
    SlideSize::Custom { cx, cy }
}

/// 讀取 `PresentationFormat`。
pub fn presentation_format_of(pkg: &Package) -> Result<Option<String>> {
    if !pkg.contains(APP_PART) {
        return Ok(None);
    }
    let doc = pkg.xml(APP_PART)?;
    Ok(doc.root.child_el("PresentationFormat").and_then(text_of))
}

/// 讀取 `Slides`（投影片頁數屬性）。
pub fn slides_of(pkg: &Package) -> Result<Option<u32>> {
    if !pkg.contains(APP_PART) {
        return Ok(None);
    }
    let doc = pkg.xml(APP_PART)?;
    Ok(doc
        .root
        .child_el("Slides")
        .and_then(text_of)
        .and_then(|v| v.parse().ok()))
}

/// 依簡報實際狀態修正 `docProps/app.xml`。
///
/// * `PresentationFormat` 依 `p:sldSz` 決定
/// * `Slides` 依 `p:sldIdLst` 的實際長度
///
/// 檔案不存在時會建立一個最小可用的 `app.xml`。
pub fn sync(pkg: &mut Package) -> Result<()> {
    let pres_doc = pkg.xml(PRESENTATION_PART)?;
    let (cx, cy, _) = pres::slide_size(&pres_doc)
        .ok_or_else(|| crate::error::Error::NodeNotFound("p:sldSz".to_owned()))?;
    let size = slide_size_from_emu(cx, cy);
    let slide_count = pres::slide_count(&pres_doc) as u32;

    let mut doc = if pkg.contains(APP_PART) {
        pkg.xml(APP_PART)?
    } else {
        Document {
            declaration: None,
            root: Element::new("Properties")
                .attr("xmlns", NS_EXTENDED)
                .attr("xmlns:vt", NS_VT),
        }
    };

    set_text(
        &mut doc.root,
        "PresentationFormat",
        presentation_format(size),
    );
    let index = doc
        .root
        .children
        .iter()
        .position(|n| matches!(n, Node::Element(e) if e.name == "PresentationFormat"));
    // `Slides` 依 schema 緊接在 `PresentationFormat` 之後；沒有就附加到最後。
    match doc.root.child_el("Slides") {
        Some(_) => set_text(&mut doc.root, "Slides", slide_count.to_string()),
        None => {
            let element = Node::Element(Element::text("Slides", slide_count.to_string()));
            match index {
                Some(i) => doc.root.children.insert(i + 1, element),
                None => doc.root.children.push(element),
            }
        }
    }

    pkg.set_xml(APP_PART, &doc)
}

fn text_of(el: &Element) -> Option<String> {
    el.children.first().and_then(|n| match n {
        Node::Text(s) => Some(s.clone()),
        _ => None,
    })
}

fn set_text(root: &mut Element, name: &str, value: impl Into<String>) {
    let value = value.into();
    if let Some(existing) = root.child_el_mut(name) {
        existing.children = vec![Node::Text(value)];
        return;
    }
    root.children
        .push(Node::Element(Element::text(name, value)));
}
