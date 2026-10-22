//! 解析 MinerU／PaddleOCR 的版面 JSON，轉成 [`n2p_layout::LayoutPage`]。
//!
//! 支援兩種實際存在的格式：
//!
//! 1. **MinerU `middle.json`**（`ppt_refiner.py` 用的）：
//!    `{ "pdf_info": [ { "page_size": [w,h], "para_blocks": [...], "discarded_blocks": [...] } ] }`
//! 2. **PaddleOCR / PP-Structure**（`ppt_creater.py` 用的）：
//!    `{ "layoutParsingResults": [ { "prunedResult": { "width", "height", "parsing_res_list": [...] } } ] }`
//!
//! 兩者都會被攤平成同一組區塊；欄位名以寬鬆方式辨識（`type`/`block_label`、
//! `bbox`/`block_bbox`、`lines[].spans[].content`）。

use n2p_core::BBox;
use n2p_layout::{Block, BlockKind, Error, LayoutPage, LayoutProvider, Line, Result, Span};
use serde_json::Value;

/// 一份已解析的版面文件。
#[derive(Debug, Clone)]
pub struct MineruDocument {
    pages: Vec<LayoutPage>,
    source_kind: SourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// MinerU `middle.json`（`pdf_info`）
    Mineru,
    /// PaddleOCR / PP-Structure（`layoutParsingResults`）
    Paddle,
}

impl MineruDocument {
    pub fn from_json_str(text: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(text)
            .map_err(|e| Error::Malformed(format!("JSON 解析失敗：{e}")))?;
        Self::from_value(&value)
    }

    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Malformed(format!("讀取 JSON 失敗：{e}")))?;
        Self::from_json_str(&text)
    }

    pub fn from_value(value: &Value) -> Result<Self> {
        if let Some(pdf_info) = value.get("pdf_info").and_then(Value::as_array) {
            let pages = pdf_info
                .iter()
                .map(parse_mineru_page)
                .collect::<Result<Vec<_>>>()?;
            return Ok(Self {
                pages,
                source_kind: SourceKind::Mineru,
            });
        }
        if let Some(results) = value.get("layoutParsingResults").and_then(Value::as_array) {
            let pages = results
                .iter()
                .map(parse_paddle_page)
                .collect::<Result<Vec<_>>>()?;
            return Ok(Self {
                pages,
                source_kind: SourceKind::Paddle,
            });
        }
        Err(Error::Malformed(
            "找不到 `pdf_info`（MinerU）或 `layoutParsingResults`（PaddleOCR）".into(),
        ))
    }

    pub fn source_kind(&self) -> SourceKind {
        self.source_kind
    }

    /// 取某一頁頁面寬度（pt）。`refine` 流程用它算 PPT 縮放比。
    pub fn page_width_pt(&self, index: usize) -> Result<f64> {
        self.page(index).map(|p| p.size_pt.0)
    }
}

impl LayoutProvider for MineruDocument {
    fn name(&self) -> &str {
        match self.source_kind {
            SourceKind::Mineru => "mineru",
            SourceKind::Paddle => "paddle",
        }
    }

    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn page(&self, index: usize) -> Result<LayoutPage> {
        self.pages.get(index).cloned().ok_or(Error::PageOutOfRange {
            index,
            count: self.pages.len(),
        })
    }
}

// ── MinerU ──────────────────────────────────────────────────────────────────

fn parse_mineru_page(value: &Value) -> Result<LayoutPage> {
    let size = value
        .get("page_size")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Malformed("頁面缺少 page_size".into()))?;
    let width = size
        .first()
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::Malformed("page_size[0] 不是數字".into()))?;
    let height = size
        .get(1)
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::Malformed("page_size[1] 不是數字".into()))?;

    let mut blocks = Vec::new();
    for key in ["para_blocks", "discarded_blocks"] {
        if let Some(list) = value.get(key).and_then(Value::as_array) {
            flatten_blocks(list, &mut blocks)?;
        }
    }
    Ok(LayoutPage {
        size_pt: (width, height),
        blocks,
    })
}

/// 遞迴攤平：MinerU 會把區塊包在 `blocks` 裡，葉節點才有 `type` 與 `bbox`。
fn flatten_blocks(list: &[Value], out: &mut Vec<Block>) -> Result<()> {
    for item in list {
        if let Some(children) = item.get("blocks").and_then(Value::as_array) {
            flatten_blocks(children, out)?;
            continue;
        }
        out.push(parse_block(item)?);
    }
    Ok(())
}

fn parse_block(item: &Value) -> Result<Block> {
    let label = item
        .get("type")
        .or_else(|| item.get("block_label"))
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Malformed("區塊缺少 type / block_label".into()))?;
    let kind = BlockKind::parse(label);

    let bbox_value = item.get("bbox").or_else(|| item.get("block_bbox"));
    let bbox = match bbox_value {
        Some(value) => parse_bbox(value)?,
        None => BBox::new(0, 0, 0, 0),
    };

    let mut lines = Vec::new();
    if let Some(list) = item.get("lines").and_then(Value::as_array) {
        for line in list {
            let mut spans = Vec::new();
            if let Some(span_list) = line.get("spans").and_then(Value::as_array) {
                for span in span_list {
                    spans.push(Span {
                        text: span
                            .get("content")
                            .or_else(|| span.get("text"))
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        image_path: span
                            .get("image_path")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    });
                }
            }
            lines.push(Line {
                spans,
                bbox: line.get("bbox").and_then(|v| parse_bbox(v).ok()),
            });
        }
    }

    // 文字優先用線性內容組合；沒有 lines 時退回 `block_content`。
    let text = if lines.is_empty() {
        item.get("block_content")
            .or_else(|| item.get("content"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    } else {
        lines.iter().map(Line::text).collect::<Vec<_>>().join("\n")
    };

    Ok(Block {
        kind,
        bbox,
        text,
        lines,
        rotation_mdeg: 0,
    })
}

// ── PaddleOCR ───────────────────────────────────────────────────────────────

fn parse_paddle_page(value: &Value) -> Result<LayoutPage> {
    let pruned = value
        .get("prunedResult")
        .ok_or_else(|| Error::Malformed("頁面缺少 prunedResult".into()))?;
    let width = pruned
        .get("width")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::Malformed("prunedResult 缺少 width".into()))?;
    let height = pruned
        .get("height")
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::Malformed("prunedResult 缺少 height".into()))?;

    let mut blocks = Vec::new();
    if let Some(list) = pruned.get("parsing_res_list").and_then(Value::as_array) {
        for item in list {
            blocks.push(parse_block(item)?);
        }
    }
    Ok(LayoutPage {
        size_pt: (width, height),
        blocks,
    })
}

fn parse_bbox(value: &Value) -> Result<BBox> {
    let array = value
        .as_array()
        .ok_or_else(|| Error::Malformed("bbox 不是陣列".into()))?;
    if array.len() < 4 {
        return Err(Error::Malformed(format!(
            "bbox 需要 4 個元素，實際 {}",
            array.len()
        )));
    }
    let number = |index: usize| -> Result<f64> {
        array[index]
            .as_f64()
            .or_else(|| array[index].as_i64().map(|v| v as f64))
            .ok_or_else(|| Error::Malformed(format!("bbox[{index}] 不是數字")))
    };
    Ok(BBox::new(
        number(0)?.floor() as i64,
        number(1)?.floor() as i64,
        number(2)?.ceil() as i64,
        number(3)?.ceil() as i64,
    ))
}

/// 從 PNG 檔名取出頁索引（0 起算），對應 `get_indices_from_png_names`。
pub fn indices_from_png_names(names: &[String]) -> Vec<usize> {
    names
        .iter()
        .filter_map(|name| {
            let base = name.rsplit('/').next().unwrap_or(name);
            let digits = base.strip_prefix("page_")?.strip_suffix(".png")?;
            digits.parse::<usize>().ok().map(|n| n - 1)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mineru_middle_json() {
        let json = r#"{
          "pdf_info": [
            {
              "page_size": [960.0, 540.0],
              "para_blocks": [
                { "type": "title", "bbox": [10, 20, 200, 40],
                  "lines": [{ "spans": [{ "content": "標題" }] }] },
                { "type": "image_body", "bbox": [30, 60, 120, 180],
                  "lines": [{ "spans": [{ "image_path": "images/a.jpg" }] }] },
                { "type": "text", "bbox": [10, 200, 300, 260],
                  "lines": [
                    { "spans": [{ "content": "第一行" }] },
                    { "spans": [{ "content": "第二行" }] }
                  ] }
              ],
              "discarded_blocks": [
                { "type": "footer", "bbox": [10, 500, 300, 520],
                  "lines": [{ "spans": [{ "content": "page 1" }] }] }
              ]
            }
          ]
        }"#;

        let doc = MineruDocument::from_json_str(json).unwrap();
        assert_eq!(doc.source_kind(), SourceKind::Mineru);
        assert_eq!(doc.page_count(), 1);

        let page = doc.page(0).unwrap();
        assert_eq!(page.size_pt, (960.0, 540.0));
        assert_eq!(
            page.blocks.len(),
            4,
            "para_blocks + discarded_blocks 都要納入"
        );
        assert_eq!(page.text_blocks().count(), 3, "title + text + footer");
        assert_eq!(page.foreground_blocks().count(), 1);
        assert_eq!(page.blocks[2].text, "第一行\n第二行");
        assert_eq!(page.blocks[1].image_path(), Some("images/a.jpg"));
        assert_eq!(page.blocks[0].kind, BlockKind::Title);
        assert_eq!(page.blocks[3].kind, BlockKind::Footer);
    }

    #[test]
    fn flattens_nested_blocks() {
        let json = r#"{
          "pdf_info": [{
            "page_size": [100, 100],
            "para_blocks": [
              { "type": "group", "bbox": [0,0,100,100],
                "blocks": [
                  { "type": "text", "bbox": [1,2,3,4], "lines": [] },
                  { "type": "text", "bbox": [5,6,7,8], "lines": [] }
                ] }
            ],
            "discarded_blocks": []
          }]
        }"#;
        let doc = MineruDocument::from_json_str(json).unwrap();
        let page = doc.page(0).unwrap();
        assert_eq!(page.blocks.len(), 2, "群組不應保留，只留葉節點");
        assert_eq!(page.blocks[0].bbox, BBox::new(1, 2, 3, 4));
    }

    #[test]
    fn parses_paddle_layout_results() {
        let json = r#"{
          "layoutParsingResults": [{
            "prunedResult": {
              "width": 800, "height": 600,
              "parsing_res_list": [
                { "block_label": "text", "block_bbox": [10, 10, 100, 50], "block_content": "內容" },
                { "block_label": "image", "block_bbox": [20, 60, 120, 160] }
              ]
            }
          }]
        }"#;
        let doc = MineruDocument::from_json_str(json).unwrap();
        assert_eq!(doc.source_kind(), SourceKind::Paddle);
        let page = doc.page(0).unwrap();
        assert_eq!(page.size_pt, (800.0, 600.0));
        assert_eq!(page.blocks.len(), 2);
        assert_eq!(page.blocks[0].text, "內容");
        assert_eq!(page.blocks[1].kind, BlockKind::Image);
    }

    #[test]
    fn rejects_unknown_shapes() {
        let error = MineruDocument::from_json_str(r#"{"foo":[]}"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("pdf_info"), "實際：{error}");
        assert!(MineruDocument::from_json_str("not json").is_err());
    }

    #[test]
    fn bbox_is_floored_and_ceiled() {
        let json = r#"{"pdf_info":[{"page_size":[10,10],"para_blocks":[
            {"type":"text","bbox":[1.7,2.2,9.1,8.8],"lines":[]}],"discarded_blocks":[]}]}"#;
        let doc = MineruDocument::from_json_str(json).unwrap();
        assert_eq!(doc.page(0).unwrap().blocks[0].bbox, BBox::new(1, 2, 10, 9));
    }

    #[test]
    fn png_name_indices() {
        let names = vec![
            "page_0001.png".to_string(),
            "/tmp/x/page_0007.png".to_string(),
            "bad.png".to_string(),
        ];
        assert_eq!(indices_from_png_names(&names), vec![0, 6]);
    }
}
