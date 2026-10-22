//! 最小 XML DOM：以「限定名稱（qualified name）」為鍵，忠實往返 OOXML part。
//!
//! 設計取捨：
//!
//! * **不做命名空間解析。** OOXML 的 `p:` / `a:` / `r:` 前綴在整份文件內固定，
//!   操作時直接用限定名稱比對（`"p:sp"`、`"a:blip"`）比先解析再重組更不容易出錯，
//!   而且序列化時不會擅自改變前綴——那會破壞 PowerPoint 的相容性。
//! * **保留註解與文字節點原樣。** 讓 `open → save → open` 的 XML 內容不變，
//!   如此才能用「往返等同」當回歸測試。
//! * **屬性不做正規化。** 順序、重複的 `xmlns` 宣告都照原樣保留或由呼叫端決定。

use crate::error::{Error, Result};
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};

/// XML 節點。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Element(Element),
    Text(String),
    Comment(String),
}

/// XML 元素。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// 限定名稱，例如 `p:sp`、`a:txBody`。
    pub name: String,
    /// 屬性（含 `xmlns:*`），順序保留。
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

/// 完整文件：宣告字串 + 根元素。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// 原始 XML 宣告（含 `<?xml ...?>`）。`None` 表示沿用預設宣告。
    pub declaration: Option<String>,
    pub root: Element,
}

impl Element {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }

    /// 鏈式設定屬性。
    pub fn attr(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attrs.push((key.into(), value.into()));
        self
    }

    /// 鏈式追加子節點。
    pub fn child(mut self, node: impl Into<Node>) -> Self {
        self.children.push(node.into());
        self
    }

    /// 追加多個子節點。
    pub fn children<I, N>(mut self, nodes: I) -> Self
    where
        I: IntoIterator<Item = N>,
        N: Into<Node>,
    {
        self.children.extend(nodes.into_iter().map(Into::into));
        self
    }

    /// 建一個只含文字的元素，例如 `<a:t>hello</a:t>`。
    pub fn text(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self::new(name).child(Node::Text(value.into()))
    }

    pub fn get_attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn set_attr(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if let Some(slot) = self.attrs.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            self.attrs.push((key, value));
        }
    }

    pub fn remove_attr(&mut self, key: &str) {
        self.attrs.retain(|(k, _)| k != key);
    }

    /// 第一個同名子元素。
    pub fn child_el(&self, name: &str) -> Option<&Element> {
        self.children.iter().find_map(|n| match n {
            Node::Element(e) if e.name == name => Some(e),
            _ => None,
        })
    }

    pub fn child_el_mut(&mut self, name: &str) -> Option<&mut Element> {
        self.children.iter_mut().find_map(|n| match n {
            Node::Element(e) if e.name == name => Some(e),
            _ => None,
        })
    }

    /// 走訪指定路徑。
    ///
    /// 第一段若等於自身名稱會被視為「從自己開始」，因此
    /// `root.path(&["p:sld", "p:cSld", "p:spTree"])` 與
    /// `root.path(&["p:cSld", "p:spTree"])` 在根為 `p:sld` 時等價。
    /// 這樣呼叫端可以一律寫完整路徑，不必記得自己是從哪一層開始。
    pub fn path(&self, names: &[&str]) -> Option<&Element> {
        let names = if names.first() == Some(&self.name.as_str()) {
            &names[1..]
        } else {
            names
        };
        let mut cur = self;
        for name in names {
            cur = cur.child_el(name)?;
        }
        Some(cur)
    }

    pub fn path_mut(&mut self, names: &[&str]) -> Option<&mut Element> {
        let skip = names.first() == Some(&self.name.as_str());
        let names: Vec<&str> = if skip {
            names[1..].to_vec()
        } else {
            names.to_vec()
        };
        let mut cur = self;
        for name in names {
            cur = cur.child_el_mut(name)?;
        }
        Some(cur)
    }

    /// 走訪指定路徑，找不到就回 `NodeNotFound`，錯誤訊息帶完整路徑。
    pub fn require_path_mut(&mut self, names: &[&str]) -> Result<&mut Element> {
        let joined = names.join("/");
        self.path_mut(names).ok_or(Error::NodeNotFound(joined))
    }

    /// 所有同名子元素。
    pub fn elements<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter_map(move |n| match n {
            Node::Element(e) if e.name == name => Some(e),
            _ => None,
        })
    }

    /// 子元素數量（不含文字與註解節點）。
    pub fn element_len(&self) -> usize {
        self.children
            .iter()
            .filter(|n| matches!(n, Node::Element(_)))
            .count()
    }

    /// 遞迴蒐集所有 `p:cNvPr` 的 `id`，用於配置下一個可用 id。
    pub fn collect_c_nv_pr_ids(&self, out: &mut Vec<u32>) {
        if self.name == "p:cNvPr" {
            if let Some(v) = self.get_attr("id").and_then(|s| s.parse::<u32>().ok()) {
                out.push(v);
            }
        }
        for child in &self.children {
            if let Node::Element(e) = child {
                e.collect_c_nv_pr_ids(out);
            }
        }
    }

    /// 在指定索引插入子節點（索引以「所有子節點」計）。
    pub fn insert_child_at(&mut self, index: usize, node: impl Into<Node>) -> Result<()> {
        let index = index.min(self.children.len());
        self.children.insert(index, node.into());
        Ok(())
    }

    /// 移除所有符合條件的直接子元素，回傳移除數量。
    pub fn retain_elements<F>(&mut self, mut keep: F) -> usize
    where
        F: FnMut(&Element) -> bool,
    {
        let before = self.children.len();
        self.children.retain(|n| match n {
            Node::Element(e) => keep(e),
            _ => true,
        });
        before - self.children.len()
    }
}

impl From<Element> for Node {
    fn from(e: Element) -> Self {
        Node::Element(e)
    }
}

impl From<&Element> for Node {
    fn from(e: &Element) -> Self {
        Node::Element(e.clone())
    }
}

impl From<String> for Node {
    fn from(s: String) -> Self {
        Node::Text(s)
    }
}

// ── 解析 ─────────────────────────────────────────────────────────────────────

fn attrs_of(start: &BytesStart<'_>) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|e| Error::UnexpectedStructure(format!("屬性解析失敗：{e}")))?;
        let key = attr.key.as_ref().to_owned();
        // `Attribute::value` 是原始未轉義字串，需自行解轉義（`&amp;` → `&`）。
        let value = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| Error::UnexpectedStructure(format!("屬性 {key} 解轉義失敗：{e}")))?
            .into_owned();
        out.push((key, value));
    }
    Ok(out)
}

/// 解析一份 XML part。
pub fn parse(bytes: &[u8]) -> Result<Document> {
    let mut reader = Reader::from_reader(bytes);
    let cfg = reader.config_mut();
    cfg.trim_text_start = false;
    cfg.trim_text_end = false;
    cfg.expand_empty_elements = false;
    cfg.check_end_names = true;

    let mut declaration: Option<String> = None;
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Decl(d) => {
                let version = d.version().ok().map(|c| c.into_owned());
                let encoding = d.encoding().and_then(|r| r.ok()).map(|c| c.into_owned());
                let standalone = d.standalone().and_then(|r| r.ok()).map(|c| c.into_owned());
                declaration = Some(build_declaration(
                    version.as_deref(),
                    encoding.as_deref(),
                    standalone.as_deref(),
                ));
            }
            Event::Start(start) => {
                let name = start.name().as_ref().to_owned();
                stack.push(Element {
                    name,
                    attrs: attrs_of(&start)?,
                    children: Vec::new(),
                });
            }
            Event::Empty(start) => {
                let name = start.name().as_ref().to_owned();
                let el = Element {
                    name,
                    attrs: attrs_of(&start)?,
                    children: Vec::new(),
                };
                push_child(&mut stack, &mut root, Node::Element(el))?;
            }
            Event::End(_) => {
                let el = stack
                    .pop()
                    .ok_or_else(|| Error::UnexpectedStructure("結束標籤多於開始標籤".into()))?;
                if stack.is_empty() {
                    if root.is_some() {
                        return Err(Error::UnexpectedStructure("文件有多個根元素".into()));
                    }
                    root = Some(el);
                } else {
                    push_child(&mut stack, &mut root, Node::Element(el))?;
                }
            }
            Event::Text(t) => {
                let raw = t.into_inner();
                let decoded = quick_xml::escape::unescape(raw.as_ref())?.into_owned();
                push_child(&mut stack, &mut root, Node::Text(decoded))?;
            }
            Event::CData(t) => {
                let raw = t.into_inner().into_owned();
                push_child(&mut stack, &mut root, Node::Text(raw))?;
            }
            Event::Comment(t) => {
                let raw = t.into_inner().into_owned();
                push_child(&mut stack, &mut root, Node::Comment(raw))?;
            }
            Event::GeneralRef(r) => {
                // quick-xml 會把 `&amp;`、`&#65;` 這類參照拆成獨立事件，此處還原成文字，
                // 讓相鄰文字節點能合併回原本的樣子。
                let resolved = match r.resolve_char_ref()? {
                    Some(ch) => ch.to_string(),
                    None => {
                        let name = r.into_inner();
                        let literal = match name.as_ref() {
                            "lt" => "<",
                            "gt" => ">",
                            "amp" => "&",
                            "quot" => "\"",
                            "apos" => "'",
                            other => {
                                return Err(Error::UnexpectedStructure(format!(
                                    "未知的 XML 實體參照：&{other};"
                                )));
                            }
                        };
                        literal.to_owned()
                    }
                };
                push_child(&mut stack, &mut root, Node::Text(resolved))?;
            }
            Event::Eof => break,
            // PI / DocType 在 OOXML part 中不應出現；出現則忽略以求穩健。
            _ => {}
        }
        buf.clear();
    }

    let root = root.ok_or_else(|| Error::UnexpectedStructure("文件沒有根元素".into()))?;
    Ok(Document { declaration, root })
}

/// 由解析出的三個屬性重組 XML 宣告字串。
fn build_declaration(
    version: Option<&str>,
    encoding: Option<&str>,
    standalone: Option<&str>,
) -> String {
    let mut out = String::from("<?xml");
    if let Some(v) = version {
        out.push_str(&format!(r#" version="{v}""#));
    }
    if let Some(e) = encoding {
        out.push_str(&format!(r#" encoding="{e}""#));
    }
    if let Some(s) = standalone {
        out.push_str(&format!(r#" standalone="{s}""#));
    }
    out.push_str("?>");
    out
}

fn push_child(stack: &mut [Element], root: &mut Option<Element>, node: Node) -> Result<()> {
    match stack.last_mut() {
        Some(parent) => {
            // 合併相鄰文字節點：解析器會把 `a &amp; b` 拆成三個事件，
            // 不合併會讓往返結果與原文不同構，也讓輸出多餘。
            if let (Node::Text(new_text), Some(Node::Text(prev))) =
                (&node, parent.children.last_mut())
            {
                prev.push_str(new_text);
                return Ok(());
            }
            parent.children.push(node);
            Ok(())
        }
        None => match node {
            Node::Element(e) if root.is_none() => {
                *root = Some(e);
                Ok(())
            }
            Node::Element(_) => Err(Error::UnexpectedStructure("文件有多個根元素".into())),
            // 根層級的文字／註解／空白（例如宣告後的換行）直接丟棄。
            _ => Ok(()),
        },
    }
}

// ── 序列化 ───────────────────────────────────────────────────────────────────

const DEFAULT_DECLARATION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;

/// 序列化一份 XML part。輸出為緊湊格式（不縮排），與 OOXML 慣例一致。
pub fn to_bytes(doc: &Document) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    out.extend_from_slice(
        doc.declaration
            .as_deref()
            .unwrap_or(DEFAULT_DECLARATION)
            .as_bytes(),
    );
    write_element(&mut out, &doc.root)?;
    Ok(out)
}

fn write_element(out: &mut Vec<u8>, el: &Element) -> Result<()> {
    let mut writer = Writer::new(out);

    let mut start = BytesStart::new(el.name.as_str());
    for (key, value) in &el.attrs {
        let escaped = quick_xml::escape::escape(value.as_str());
        start.push_attribute((key.as_str(), escaped.as_ref()));
    }

    if el.children.is_empty() {
        writer.write_event(Event::Empty(start))?;
        return Ok(());
    }

    writer.write_event(Event::Start(start))?;
    for child in &el.children {
        match child {
            Node::Element(e) => write_element(writer.get_mut(), e)?,
            Node::Text(t) => {
                let escaped = quick_xml::escape::escape(t.as_str());
                writer.write_event(Event::Text(BytesText::from_escaped(escaped.as_ref())))?;
            }
            Node::Comment(c) => {
                writer.write_event(Event::Comment(BytesText::from_escaped(c.as_str())))?;
            }
        }
    }
    writer.write_event(Event::End(BytesEnd::new(el.name.as_str())))?;
    Ok(())
}

/// 遞迴改寫所有元素的屬性。
///
/// 用於重新編號關聯 id（`r:embed`、`r:id` 等）與 part 目標路徑。
pub fn rewrite_attributes<F>(el: &mut Element, f: &mut F)
where
    F: FnMut(&str, &mut String),
{
    for (key, value) in el.attrs.iter_mut() {
        f(key, value);
    }
    for child in el.children.iter_mut() {
        if let Node::Element(e) = child {
            rewrite_attributes(e, f);
        }
    }
}

/// 移除所有 `xmlns*` 屬性，僅用於比對（不影響實際輸出）。
///
/// Spire 會在 `a:blip` 上重複宣告 `xmlns:r`（根層已宣告），語意相同但字面不同。
pub fn normalize_for_compare(el: &Element) -> Element {
    let mut clone = el.clone();
    clone.attrs.retain(|(k, _)| !k.starts_with("xmlns"));
    clone.children = clone
        .children
        .into_iter()
        .map(|n| match n {
            Node::Element(e) => Node::Element(normalize_for_compare(&e)),
            other => other,
        })
        .collect();
    clone
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_structure() {
        let src = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:p="x"><p:cSld name=""><p:sp><a:noFill/><a:t>a &amp; b</a:t></p:sp></p:cSld></p:sld>"#;
        let doc = parse(src).expect("parse");
        assert_eq!(doc.root.name, "p:sld");
        let out = to_bytes(&doc).expect("write");
        let doc2 = parse(&out).expect("reparse");
        assert_eq!(doc, doc2);
        // 文字節點的解轉義與再轉義需對稱
        let text = doc
            .root
            .path(&["p:cSld", "p:sp"])
            .unwrap()
            .child_el("a:t")
            .unwrap();
        assert_eq!(text.children[0], Node::Text("a & b".into()));
    }

    #[test]
    fn empty_element_stays_empty() {
        let doc = parse(br#"<a><b/></a>"#).unwrap();
        let out = to_bytes(&doc).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("<b/>"), "取得 {text}");
    }

    #[test]
    fn normalize_drops_xmlns() {
        let doc = parse(br#"<a:blip xmlns:r="http://x" r:embed="rId2"/>"#).unwrap();
        let n = normalize_for_compare(&doc.root);
        assert_eq!(n.attrs, vec![("r:embed".to_string(), "rId2".to_string())]);
    }
}
