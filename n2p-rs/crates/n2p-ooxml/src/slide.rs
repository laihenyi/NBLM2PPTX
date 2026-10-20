//! 投影片層級操作：形狀插入、置底、背景設定、依名稱刪除。

use crate::error::{Error, Result};
use crate::xml::{Document, Element, Node};

/// `p:sld` → `p:cSld` → `p:spTree` 的路徑。
pub const SP_TREE_PATH: [&str; 3] = ["p:sld", "p:cSld", "p:spTree"];

/// 取得可變的 `p:spTree`。
pub fn sp_tree_mut(doc: &mut Document) -> Result<&mut Element> {
    doc.root.require_path_mut(&SP_TREE_PATH)
}

/// 下一個可用的形狀 id。
///
/// 以整份投影片所有 `p:cNvPr/@id` 的最大值 +1 計算。
/// `spTree` 群組本身固定佔用 id=1，因此第一個形狀自然是 2 —— 與 Spire 一致。
pub fn next_shape_id(doc: &Document) -> u32 {
    let mut ids = Vec::new();
    doc.root.collect_c_nv_pr_ids(&mut ids);
    ids.into_iter().max().map_or(1, |max| max.saturating_add(1))
}

/// 把形狀附加到 `spTree` 末端（最上層）。
///
/// 對應 Spire 的 `slide.Shapes.AppendShape(...)` / `AppendEmbedImageByPath(...)`：
/// 新形狀出現在最後，也就是視覺上的最上層。
pub fn append_shape(doc: &mut Document, shape: Element) -> Result<()> {
    let tree = sp_tree_mut(doc)?;
    tree.children.push(Node::Element(shape));
    Ok(())
}

/// 把形狀插到 `spTree` 的**最底層**（`ZOrderPosition = 0`）。
///
/// `spTree` 的前兩個子節點固定是 `p:nvGrpSpPr` 與 `p:grpSpPr`，
/// 因此「第一個形狀位置」是索引 2。若範本缺少 `p:grpSpPr`，則插在索引 0。
pub fn insert_shape_at_bottom(doc: &mut Document, shape: Element) -> Result<()> {
    let tree = sp_tree_mut(doc)?;
    let index = tree
        .children
        .iter()
        .position(|n| matches!(n, Node::Element(e) if e.name == "p:grpSpPr"))
        .map_or(0, |i| i + 1);
    tree.children.insert(index, Node::Element(shape));
    Ok(())
}

/// 設定背景，並移除既有的 `p:bg`。
///
/// `p:bg` 必須是 `p:cSld` 的第一個子節點（在 `p:spTree` 之前）。
pub fn set_background(doc: &mut Document, background: Element) -> Result<()> {
    let c_sld = doc.root.require_path_mut(&["p:sld", "p:cSld"])?;
    c_sld.retain_elements(|e| e.name != "p:bg");
    c_sld.children.insert(0, Node::Element(background));
    Ok(())
}

/// 移除既有的 `p:bg`，回傳是否真的移除。
pub fn clear_background(doc: &mut Document) -> Result<bool> {
    let c_sld = doc.root.require_path_mut(&["p:sld", "p:cSld"])?;
    Ok(c_sld.retain_elements(|e| e.name != "p:bg") > 0)
}

/// 目前是否有自訂背景。
pub fn background(doc: &Document) -> Option<&Element> {
    doc.root.path(&["p:sld", "p:cSld"])?.child_el("p:bg")
}

/// 所有 `p:sp` 的 `name`，按文件順序。
pub fn shape_names(doc: &Document) -> Result<Vec<String>> {
    let tree = doc
        .root
        .path(&SP_TREE_PATH)
        .ok_or_else(|| Error::NodeNotFound(SP_TREE_PATH.join("/")))?;
    Ok(tree
        .elements("p:sp")
        .filter_map(|sp| {
            sp.path(&["p:nvSpPr", "p:cNvPr"])
                .and_then(|e| e.get_attr("name"))
        })
        .map(str::to_owned)
        .collect())
}

/// 依 `p:cNvPr/@name` 刪除形狀，回傳刪除數量。
///
/// 這個函式是 Python 版 `clean_ppt` 的等價物。**Rust 版不需要用它** ——
/// 因為自己產生 XML 就不會有 Spire 的評估水印。保留它是為了能處理
/// 「讀入既有的、由 Spire 產生的 PPTX」這種情況（例如 `refine_ppt` 的整併流程）。
pub fn remove_shapes_named(doc: &mut Document, name: &str) -> Result<usize> {
    let tree = sp_tree_mut(doc)?;
    Ok(tree.retain_elements(|e| {
        let matches = e.name == "p:sp"
            && e.path(&["p:nvSpPr", "p:cNvPr"])
                .and_then(|n| n.get_attr("name"))
                == Some(name);
        !matches
    }))
}

/// 列出 `spTree` 下所有形狀節點的種類（`p:sp` / `p:pic` / `p:graphicFrame` / …）。
pub fn shape_kinds(doc: &Document) -> Result<Vec<&str>> {
    let tree = doc
        .root
        .path(&SP_TREE_PATH)
        .ok_or_else(|| Error::NodeNotFound(SP_TREE_PATH.join("/")))?;
    Ok(tree
        .children
        .iter()
        .filter_map(|n| match n {
            Node::Element(e) if e.name != "p:nvGrpSpPr" && e.name != "p:grpSpPr" => {
                Some(e.name.as_str())
            }
            _ => None,
        })
        .collect())
}

// ── refine_ppt 需要的操作 ────────────────────────────────────────────────────

use std::collections::HashMap;

/// 依對照表改寫所有 `r:*` 關聯屬性（`r:embed`、`r:id`、`r:link` …）。
///
/// 用於把來源簡報的 `rIdN` 換成目標封裝裡重新配置的 id。
pub fn rewrite_rel_ids(doc: &mut Document, mapping: &HashMap<String, String>) {
    crate::xml::rewrite_attributes(&mut doc.root, &mut |key, value| {
        if key.starts_with("r:") {
            if let Some(new_id) = mapping.get(value.as_str()) {
                *value = new_id.clone();
            }
        }
    });
}

/// 把所有文字 run 的拉丁字型統一成 `typeface`，回傳被改寫的 run 數量。
///
/// 對應 Spire 的：
/// ```text
/// for textRange in paragraph.TextRanges: textRange.LatinFont = TextFont(font_name)
/// ```
/// 這裡走整份文件，因此不需要先列舉段落。`a:rPr`（run）、`a:endParaRPr`（段落結尾）
/// 與 `a:defRPr`（預設）三種都處理。
pub fn set_all_fonts(doc: &mut Document, typeface: &str) -> usize {
    let mut touched = 0usize;
    fn walk(el: &mut Element, typeface: &str, touched: &mut usize) {
        if matches!(el.name.as_str(), "a:rPr" | "a:endParaRPr" | "a:defRPr") {
            match el.child_el_mut("a:latin") {
                Some(latin) => {
                    if latin.get_attr("typeface") != Some(typeface) {
                        latin.set_attr("typeface", typeface);
                        *touched += 1;
                    }
                }
                None => {
                    // `a:latin` 在 `a:rPr` 的 schema 順序中排在填色之後、`a:ea` 之前；
                    // 追加到最後在 PowerPoint 與 LibreOffice 都可接受。
                    el.children.push(Node::Element(
                        Element::new("a:latin").attr("typeface", typeface),
                    ));
                    *touched += 1;
                }
            }
        }
        for child in el.children.iter_mut() {
            if let Node::Element(e) = child {
                walk(e, typeface, touched);
            }
        }
    }
    walk(&mut doc.root, typeface, &mut touched);
    touched
}

/// 移除 `spTree` 中所有非文字框的形狀（保留 `p:sp` 與兩個群組屬性節點）。
///
/// 對應 `refine_ppt` 的：
/// ```text
/// if "IAutoShape" not in str(type(shape)): slide.Shapes.RemoveAt(i)
/// ```
/// 注意：Spire 的 `Shapes` 索引只涵蓋形狀，不涵蓋 `p:nvGrpSpPr` / `p:grpSpPr`，
/// 因此這裡也一併保留那兩個節點。
pub fn remove_non_text_shapes(doc: &mut Document) -> Result<usize> {
    let tree = sp_tree_mut(doc)?;
    Ok(tree.retain_elements(|e| {
        !matches!(
            e.name.as_str(),
            "p:pic" | "p:graphicFrame" | "p:grpSp" | "p:cxnSp"
        )
    }))
}

/// 既有背景圖的關聯 id：`p:cSld/p:bg/p:bgPr/a:blipFill/a:blip/@r:embed`。
pub fn background_blip_rid(doc: &Document) -> Option<&str> {
    doc.root
        .path(&["p:sld", "p:cSld", "p:bg", "p:bgPr", "a:blipFill", "a:blip"])
        .and_then(|e| e.get_attr("r:embed"))
}

/// 列舉所有文字框的文字內容（`p:sp` 的 `a:t` 串接），用於 `refine_ppt` 的 IOU 比對。
pub fn text_shapes(doc: &Document) -> Result<Vec<TextBoxInfo>> {
    let tree = doc
        .root
        .path(&SP_TREE_PATH)
        .ok_or_else(|| Error::NodeNotFound(SP_TREE_PATH.join("/")))?;
    Ok(tree
        .elements("p:sp")
        .filter_map(|sp| {
            let c_nv_pr = sp.path(&["p:nvSpPr", "p:cNvPr"])?;
            let off = sp.path(&["p:spPr", "a:xfrm", "a:off"])?;
            let ext = sp.path(&["p:spPr", "a:xfrm", "a:ext"])?;
            let text = sp
                .path(&["p:txBody"])?
                .elements("a:p")
                .flat_map(|p| p.elements("a:r"))
                .filter_map(|r| r.child_el("a:t"))
                .filter_map(|t| t.children.first())
                .filter_map(|n| match n {
                    Node::Text(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect::<String>();
            Some(TextBoxInfo {
                name: c_nv_pr.get_attr("name").unwrap_or_default().to_owned(),
                text,
                x_emu: off.get_attr("x")?.parse().ok()?,
                y_emu: off.get_attr("y")?.parse().ok()?,
                cx_emu: ext.get_attr("cx")?.parse().ok()?,
                cy_emu: ext.get_attr("cy")?.parse().ok()?,
            })
        })
        .collect())
}

/// 一個文字框的位置與內容摘要（座標單位 EMU）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBoxInfo {
    pub name: String,
    pub text: String,
    pub x_emu: i64,
    pub y_emu: i64,
    pub cx_emu: i64,
    pub cy_emu: i64,
}

impl TextBoxInfo {
    /// `[left, top, right, bottom]`，與 `refine_ppt` 的 `compute_ious` 介面一致。
    pub fn bounds(&self) -> [f64; 4] {
        [
            self.x_emu as f64,
            self.y_emu as f64,
            (self.x_emu + self.cx_emu) as f64,
            (self.y_emu + self.cy_emu) as f64,
        ]
    }
}
