//! `ppt/presentation.xml`：畫布尺寸與投影片清單。

use crate::emu::{SlideSize, emu_from_pt, pt_from_emu};
use crate::error::{Error, Result};
use crate::xml::{Document, Element, Node};

pub const PRESENTATION_PART: &str = "ppt/presentation.xml";

/// 設定畫布尺寸。
///
/// 對應 `presentation.SlideSize.Type = ...`。`sldSz` 是 `p:presentation` 的
/// 直接子節點；已存在就改屬性，不存在就補一個（位置依 ECMA-376 建議放在 `sldIdLst` 之後）。
pub fn set_slide_size(doc: &mut Document, size: SlideSize) -> Result<()> {
    let (cx, cy) = size.emu();
    let existing = doc
        .root
        .children
        .iter()
        .position(|n| matches!(n, Node::Element(e) if e.name == "p:sldSz"));

    let mut element = Element::new("p:sldSz")
        .attr("cx", cx.to_string())
        .attr("cy", cy.to_string());
    if let Some(type_attr) = size.type_attr() {
        element.set_attr("type", type_attr);
    }

    match existing {
        Some(index) => {
            doc.root.children[index] = Node::Element(element);
        }
        None => {
            let insert_at = doc
                .root
                .children
                .iter()
                .position(|n| matches!(n, Node::Element(e) if e.name == "p:sldIdLst"))
                .map_or(doc.root.children.len(), |i| i + 1);
            doc.root.children.insert(insert_at, Node::Element(element));
        }
    }
    Ok(())
}

/// 讀取畫布尺寸：`(cx, cy, type_attr)`，單位 EMU。
pub fn slide_size(doc: &Document) -> Option<(i64, i64, Option<String>)> {
    let sz = doc.root.child_el("p:sldSz")?;
    let cx = sz.get_attr("cx")?.parse().ok()?;
    let cy = sz.get_attr("cy")?.parse().ok()?;
    Some((cx, cy, sz.get_attr("type").map(str::to_owned)))
}

/// 以點為單位回傳畫布尺寸。
pub fn slide_size_pt(doc: &Document) -> Option<(f64, f64)> {
    slide_size(doc).map(|(cx, cy, _)| (pt_from_emu(cx), pt_from_emu(cy)))
}

/// 依 PDF 頁面點數設定畫布，回傳實際採用的 `SlideSize`。
///
/// 等價於 `setup_presentation(pdf_size)` 的 `strategy == "diff"` 分支。
pub fn apply_pdf_page_size(doc: &mut Document, width_pt: f64, height_pt: f64) -> Result<SlideSize> {
    let size = SlideSize::from_ratio(width_pt, height_pt);
    set_slide_size(doc, size)?;
    Ok(size)
}

/// 以點數自訂畫布（對應 `SlideSizeType.Custom`）。
pub fn set_custom_size_pt(doc: &mut Document, width_pt: f64, height_pt: f64) -> Result<()> {
    set_slide_size(
        doc,
        SlideSize::Custom {
            cx: emu_from_pt(width_pt),
            cy: emu_from_pt(height_pt),
        },
    )
}

/// `p:sldIdLst` 中的所有 `r:id`，依簡報順序。
pub fn slide_rel_ids(doc: &Document) -> Result<Vec<String>> {
    let list = doc
        .root
        .child_el("p:sldIdLst")
        .ok_or_else(|| Error::NodeNotFound("p:sldIdLst".to_owned()))?;
    Ok(list
        .elements("p:sldId")
        .filter_map(|e| e.get_attr("r:id"))
        .map(str::to_owned)
        .collect())
}

/// 在 `p:sldIdLst` 末端追加一筆投影片參照。
pub fn push_slide_rel(doc: &mut Document, rid: &str) -> Result<u32> {
    let new_id = next_slide_id(doc);
    let list = doc
        .root
        .child_el_mut("p:sldIdLst")
        .ok_or_else(|| Error::NodeNotFound("p:sldIdLst".to_owned()))?;
    list.children.push(Node::Element(
        Element::new("p:sldId")
            .attr("r:id", rid)
            .attr("id", new_id.to_string()),
    ));
    Ok(new_id)
}

/// 下一個可用的 `p:sldId/@id`。
///
/// 依 OOXML 規範此值必須介於 256 與 2147483647 之間且唯一。
pub fn next_slide_id(doc: &Document) -> u32 {
    let max = doc
        .root
        .child_el("p:sldIdLst")
        .map(|list| {
            list.elements("p:sldId")
                .filter_map(|e| e.get_attr("id").and_then(|v| v.parse::<u32>().ok()))
                .max()
                .unwrap_or(255)
        })
        .unwrap_or(255);
    max.saturating_add(1)
}

/// 追加一筆 `p:sldMasterId`，回傳使用的 id。
pub fn add_master_id(doc: &mut Document, rid: &str) -> Result<u32> {
    let new_id = next_master_id(doc);
    if doc.root.child_el("p:sldMasterIdLst").is_none() {
        // 依 ECMA-376，`p:sldMasterIdLst` 必須是 `p:presentation` 的第一個子節點。
        doc.root
            .children
            .insert(0, Node::Element(Element::new("p:sldMasterIdLst")));
    }
    let list = doc
        .root
        .child_el_mut("p:sldMasterIdLst")
        .ok_or_else(|| Error::NodeNotFound("p:sldMasterIdLst".to_owned()))?;
    list.children.push(Node::Element(
        Element::new("p:sldMasterId")
            .attr("r:id", rid)
            .attr("id", new_id.to_string()),
    ));
    Ok(new_id)
}

/// 下一個可用的 `p:sldMasterId/@id`（必須 ≥ 2147483648）。
pub fn next_master_id(doc: &Document) -> u32 {
    let max = doc
        .root
        .child_el("p:sldMasterIdLst")
        .map(|list| {
            list.elements("p:sldMasterId")
                .filter_map(|e| e.get_attr("id").and_then(|v| v.parse::<u32>().ok()))
                .max()
                .unwrap_or(2_147_483_647)
        })
        .unwrap_or(2_147_483_647);
    max.saturating_add(1)
}

/// 移除指定 `r:id` 的 `p:sldId`，回傳是否真的移除。
pub fn remove_slide_rel(doc: &mut Document, rid: &str) -> bool {
    let Some(list) = doc.root.child_el_mut("p:sldIdLst") else {
        return false;
    };
    let before = list.children.len();
    list.children.retain(|n| match n {
        Node::Element(e) => !(e.name == "p:sldId" && e.get_attr("r:id") == Some(rid)),
        _ => true,
    });
    before != list.children.len()
}

/// 依投影片索引移除 `p:sldId`（呼叫端負責同步 rels 與 part）。
pub fn remove_slide_rel_at(doc: &mut Document, index: usize) -> Option<String> {
    let list = doc.root.child_el_mut("p:sldIdLst")?;
    let mut seen = 0usize;
    let mut target = None;
    for node in &list.children {
        if let Node::Element(e) = node {
            if e.name == "p:sldId" {
                if seen == index {
                    target = e.get_attr("r:id").map(str::to_owned);
                    break;
                }
                seen += 1;
            }
        }
    }
    let rid = target?;
    remove_slide_rel(doc, &rid);
    Some(rid)
}

/// 投影片頁數。
pub fn slide_count(doc: &Document) -> usize {
    doc.root
        .child_el("p:sldIdLst")
        .map(|list| list.elements("p:sldId").count())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Document {
        let src = br#"<p:presentation><p:sldIdLst><p:sldId r:id="rId7" id="257"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000" type="screen4x3"/></p:presentation>"#;
        crate::xml::parse(src).unwrap()
    }

    #[test]
    fn set_slide_size_replaces_attrs() {
        let mut doc = sample();
        set_slide_size(&mut doc, SlideSize::Screen16x9).unwrap();
        assert_eq!(
            slide_size(&doc),
            Some((12_192_000, 6_858_000, Some("screen16x9".to_owned())))
        );
    }

    #[test]
    fn custom_size_has_no_type_attr() {
        let mut doc = sample();
        set_slide_size(&mut doc, SlideSize::Custom { cx: 100, cy: 200 }).unwrap();
        assert_eq!(slide_size(&doc), Some((100, 200, None)));
    }

    #[test]
    fn next_slide_id_after_existing() {
        let doc = sample();
        assert_eq!(next_slide_id(&doc), 258);
        assert_eq!(slide_rel_ids(&doc).unwrap(), vec!["rId7".to_string()]);
    }
}
