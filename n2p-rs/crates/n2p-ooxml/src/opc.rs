//! OPC 容器：zip + `[Content_Types].xml` + `_rels`。
//!
//! 這裡只做 PPTX 實際會用到的最小集合，刻意不實作完整的 OPC 規格：
//!
//! * part 名稱一律用「不含前導 `/` 的絕對路徑」當鍵（`ppt/slides/slide1.xml`）。
//! * 關聯（relationship）目標以相對於來源 part 所在目錄的形式保存，寫入時照原樣輸出。
//! * `[Content_Types].xml` 只維護 `Default`（副檔名）與 `Override`（具名 part）兩種。

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use crate::error::{Error, Result};
use crate::xml::{self, Document};

pub const CT_PART: &str = "[Content_Types].xml";
pub const RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
pub const REL_TYPE_IMAGE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
pub const REL_TYPE_SLIDE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
pub const REL_TYPE_SLIDE_LAYOUT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
pub const REL_TYPE_SLIDE_MASTER: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
pub const REL_TYPE_THEME: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
pub const CT_SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
pub const CT_SLIDE_LAYOUT: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml";
pub const CT_SLIDE_MASTER: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml";
pub const CT_THEME: &str = "application/vnd.openxmlformats-officedocument.theme+xml";

/// 影像副檔名 → OPC content type。
pub fn image_content_type(extension: &str) -> Option<&'static str> {
    match extension
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        "tif" | "tiff" => Some("image/tiff"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

/// 一組關聯。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Relationships {
    pub items: Vec<Relationship>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    pub id: String,
    pub rel_type: String,
    /// 相對於來源 part 目錄的目標路徑。
    pub target: String,
    pub target_mode: Option<String>,
}

impl Relationships {
    pub fn from_document(doc: &Document) -> Self {
        let items = doc
            .root
            .elements("Relationship")
            .map(|e| Relationship {
                id: e.get_attr("Id").unwrap_or_default().to_owned(),
                rel_type: e.get_attr("Type").unwrap_or_default().to_owned(),
                target: e.get_attr("Target").unwrap_or_default().to_owned(),
                target_mode: e.get_attr("TargetMode").map(str::to_owned),
            })
            .collect();
        Self { items }
    }

    pub fn to_document(&self) -> Document {
        let mut root = xml::Element::new("Relationships").attr("xmlns", RELS_NS);
        for rel in &self.items {
            let mut e = xml::Element::new("Relationship")
                .attr("Id", rel.id.as_str())
                .attr("Type", rel.rel_type.as_str())
                .attr("Target", rel.target.as_str());
            if let Some(mode) = &rel.target_mode {
                e.set_attr("TargetMode", mode.as_str());
            }
            root = root.child(e);
        }
        Document {
            declaration: None,
            root,
        }
    }

    /// 下一個可用的 `rIdN`。Spire 與 PowerPoint 都採單調遞增，這裡照做。
    pub fn next_rid(&self) -> String {
        let max = self
            .items
            .iter()
            .filter_map(|r| r.id.strip_prefix("rId").and_then(|n| n.parse::<u32>().ok()))
            .max()
            .unwrap_or(0);
        format!("rId{}", max + 1)
    }

    pub fn target_of(&self, rid: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|r| r.id == rid)
            .map(|r| r.target.as_str())
    }

    pub fn first_of_type(&self, rel_type: &str) -> Option<&Relationship> {
        self.items.iter().find(|r| r.rel_type == rel_type)
    }

    pub fn push(&mut self, rel: Relationship) {
        self.items.push(rel);
    }
}

/// 一個 OPC 封裝（＝一個 .pptx 檔）。
#[derive(Debug, Clone)]
pub struct Package {
    parts: BTreeMap<String, Vec<u8>>,
}

impl Package {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let mut parts = BTreeMap::new();
        for idx in 0..archive.len() {
            let mut entry = archive.by_index(idx)?;
            let name = entry.name()?.into_owned();
            if name.ends_with('/') {
                continue; // 目錄項
            }
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            parts.insert(name, buf);
        }
        if !parts.contains_key(CT_PART) {
            return Err(Error::PartNotFound(CT_PART.to_owned()));
        }
        Ok(Self { parts })
    }

    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let bytes = self.to_bytes()?;
        if let Some(parent) = path.as_ref().parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, bytes)?;
        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            for (name, data) in &self.parts {
                let options: zip::write::SimpleFileOptions =
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated);
                writer.start_file(name.as_str(), options)?;
                writer.write_all(data)?;
            }
            writer.finish()?;
        }
        Ok(cursor.into_inner())
    }

    pub fn part_names(&self) -> impl Iterator<Item = &str> {
        self.parts.keys().map(String::as_str)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.parts.contains_key(name)
    }

    pub fn part(&self, name: &str) -> Result<&[u8]> {
        self.parts
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| Error::PartNotFound(name.to_owned()))
    }

    pub fn set_part(&mut self, name: impl Into<String>, data: Vec<u8>) {
        self.parts.insert(name.into(), data);
    }

    pub fn remove_part(&mut self, name: &str) -> Option<Vec<u8>> {
        self.parts.remove(name)
    }

    /// 讀取並解析一份 XML part。
    pub fn xml(&self, name: &str) -> Result<Document> {
        xml::parse(self.part(name)?)
    }

    /// 序列化並寫回一份 XML part。
    pub fn set_xml(&mut self, name: &str, doc: &Document) -> Result<()> {
        let bytes = xml::to_bytes(doc)?;
        self.set_part(name, bytes);
        Ok(())
    }

    // ── [Content_Types].xml ─────────────────────────────────────────────────

    fn content_types_mut(&mut self) -> Result<Document> {
        self.xml(CT_PART)
    }

    fn save_content_types(&mut self, doc: &Document) -> Result<()> {
        self.set_xml(CT_PART, doc)
    }

    /// 確保某個副檔名有 `Default` 註冊（例如 `png` → `image/png`）。
    pub fn ensure_default_content_type(
        &mut self,
        extension: &str,
        content_type: &str,
    ) -> Result<()> {
        let mut doc = self.content_types_mut()?;
        let exists = doc.root.elements("Default").any(|e| {
            e.get_attr("Extension")
                .is_some_and(|v| v.eq_ignore_ascii_case(extension))
        });
        if !exists {
            let el = xml::Element::new("Default")
                .attr("Extension", extension)
                .attr("ContentType", content_type);
            // OOXML 慣例把 Default 放在最前面；順序不影響合法性，但保持一致較好讀。
            doc.root.children.insert(0, xml::Node::Element(el));
            self.save_content_types(&doc)?;
        }
        Ok(())
    }

    /// 確保某個 part 有 `Override` 註冊。
    pub fn ensure_override_content_type(
        &mut self,
        part_name: &str,
        content_type: &str,
    ) -> Result<()> {
        let mut doc = self.content_types_mut()?;
        let key = format!("/{part_name}");
        let exists = doc
            .root
            .elements("Override")
            .any(|e| e.get_attr("PartName") == Some(key.as_str()));
        if !exists {
            let el = xml::Element::new("Override")
                .attr("PartName", key)
                .attr("ContentType", content_type);
            doc.root.children.push(xml::Node::Element(el));
            self.save_content_types(&doc)?;
        }
        Ok(())
    }

    /// 移除某個 part 的 `Override`。
    pub fn remove_override_content_type(&mut self, part_name: &str) -> Result<()> {
        let mut doc = self.content_types_mut()?;
        let key = format!("/{part_name}");
        doc.root.retain_elements(|e| {
            !(e.name == "Override" && e.get_attr("PartName") == Some(key.as_str()))
        });
        self.save_content_types(&doc)
    }

    // ── 關聯 ────────────────────────────────────────────────────────────────

    /// 某個 part 的 `.rels` part 名稱。`""` 代表封裝根。
    pub fn rels_part_name(host_part: &str) -> String {
        match host_part.rsplit_once('/') {
            Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
            None if host_part.is_empty() => "_rels/.rels".to_owned(),
            None => format!("_rels/{host_part}.rels"),
        }
    }

    pub fn rels(&self, host_part: &str) -> Result<Relationships> {
        let name = Self::rels_part_name(host_part);
        match self.parts.get(&name) {
            Some(_) => Ok(Relationships::from_document(&self.xml(&name)?)),
            None => Ok(Relationships::default()),
        }
    }

    pub fn set_rels(&mut self, host_part: &str, rels: &Relationships) -> Result<()> {
        let name = Self::rels_part_name(host_part);
        self.set_xml(&name, &rels.to_document())
    }

    /// 把相對目標解析成絕對 part 名稱。
    ///
    /// OOXML 的 `Target` 相對於來源 part 的目錄，可能含 `../`。
    pub fn resolve_target(host_part: &str, target: &str) -> String {
        if let Some(stripped) = target.strip_prefix('/') {
            return stripped.to_owned();
        }
        let base: Vec<&str> = match host_part.rsplit_once('/') {
            Some((dir, _)) => dir.split('/').collect(),
            None => Vec::new(),
        };
        let mut segments: Vec<&str> = base;
        for segment in target.split('/') {
            match segment {
                "." | "" => {}
                ".." => {
                    segments.pop();
                }
                other => segments.push(other),
            }
        }
        segments.join("/")
    }

    /// 相對路徑（把絕對 part 名稱轉成從 `host_part` 目錄出發的目標）。
    pub fn relative_target(host_part: &str, target_part: &str) -> String {
        let base: Vec<&str> = match host_part.rsplit_once('/') {
            Some((dir, _)) => dir.split('/').collect(),
            None => Vec::new(),
        };
        let target: Vec<&str> = target_part.split('/').collect();
        let common = base
            .iter()
            .zip(target.iter())
            .take_while(|(a, b)| a == b)
            .count();
        let mut out: Vec<String> = Vec::new();
        for _ in common..base.len() {
            out.push("..".to_owned());
        }
        out.extend(target[common..].iter().map(|s| (*s).to_owned()));
        out.join("/")
    }

    // ── 影像 ────────────────────────────────────────────────────────────────

    /// 新增一張影像並掛到指定 part 的關聯上，回傳 `rId`。
    ///
    /// 這是 `Images.AppendStream` + `PictureFill.Picture.EmbedImage` 的等價操作。
    pub fn add_image_for(
        &mut self,
        host_part: &str,
        data: Vec<u8>,
        extension: &str,
    ) -> Result<String> {
        let extension = extension.trim_start_matches('.').to_ascii_lowercase();
        let content_type = image_content_type(&extension)
            .ok_or_else(|| Error::OutOfRange(format!("不支援的影像格式：{extension}")))?;

        let image_part = self.alloc_media_part(&extension);
        self.set_part(image_part.clone(), data);
        self.ensure_default_content_type(&extension, content_type)?;

        let mut rels = self.rels(host_part)?;
        let rid = rels.next_rid();
        let target = Self::relative_target(host_part, &image_part);
        rels.push(Relationship {
            id: rid.clone(),
            rel_type: REL_TYPE_IMAGE.to_owned(),
            target,
            target_mode: None,
        });
        self.set_rels(host_part, &rels)?;
        Ok(rid)
    }

    /// 配置一個未被佔用的 `ppt/media/imageN.ext` 名稱（不寫入內容）。
    pub fn alloc_media_part(&self, extension: &str) -> String {
        let extension = extension.trim_start_matches('.').to_ascii_lowercase();
        let mut index = 1u32;
        loop {
            let candidate = format!("ppt/media/image{index}.{extension}");
            if !self.parts.contains_key(&candidate) {
                return candidate;
            }
            index += 1;
        }
    }

    /// 配置一個未被佔用的 `ppt/slides/slideN.xml` 名稱。
    pub fn alloc_slide_part(&self) -> String {
        let mut index = 1u32;
        loop {
            let candidate = format!("ppt/slides/slide{index}.xml");
            if !self.parts.contains_key(&candidate) {
                return candidate;
            }
            index += 1;
        }
    }

    /// 配置一組未被佔用的 part 名稱，規則為 `<dir>/<prefix>N<ext>`。
    ///
    /// `existing_start` 用來指定起始編號（例如從既有最大編號加一）。
    pub fn alloc_numbered_part(&self, dir: &str, prefix: &str, extension: &str) -> String {
        let mut index = 1u32;
        loop {
            let candidate = format!("{dir}/{prefix}{index}.{extension}");
            if !self.parts.contains_key(&candidate) {
                return candidate;
            }
            index += 1;
        }
    }

    /// 某個目錄下既有的最大編號（`<dir>/<prefix>N<ext>` 形式）。
    pub fn max_numbered_index(&self, dir: &str, prefix: &str, extension: &str) -> u32 {
        let marker = format!("{dir}/{prefix}");
        let suffix = format!(".{extension}");
        self.parts
            .keys()
            .filter_map(|name| {
                let rest = name.strip_prefix(marker.as_str())?;
                let digits = rest.strip_suffix(suffix.as_str())?;
                digits.parse::<u32>().ok()
            })
            .max()
            .unwrap_or(0)
    }

    /// 直接指定既有影像 part 掛關聯（避免重複寫入同一張圖）。
    pub fn relate_existing(
        &mut self,
        host_part: &str,
        rel_type: &str,
        target_part: &str,
    ) -> Result<String> {
        let mut rels = self.rels(host_part)?;
        let target = Self::relative_target(host_part, target_part);
        if let Some(existing) = rels
            .items
            .iter()
            .find(|r| r.rel_type == rel_type && r.target == target)
        {
            return Ok(existing.id.clone());
        }
        let rid = rels.next_rid();
        rels.push(Relationship {
            id: rid.clone(),
            rel_type: rel_type.to_owned(),
            target,
            target_mode: None,
        });
        self.set_rels(host_part, &rels)?;
        Ok(rid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rels_part_name_layout() {
        assert_eq!(Package::rels_part_name(""), "_rels/.rels");
        assert_eq!(
            Package::rels_part_name("ppt/presentation.xml"),
            "ppt/_rels/presentation.xml.rels"
        );
        assert_eq!(
            Package::rels_part_name("ppt/slides/slide1.xml"),
            "ppt/slides/_rels/slide1.xml.rels"
        );
    }

    #[test]
    fn resolve_and_relativize_are_inverse() {
        let host = "ppt/slides/slide1.xml";
        let target = "ppt/media/image3.png";
        let rel = Package::relative_target(host, target);
        assert_eq!(rel, "../media/image3.png");
        assert_eq!(Package::resolve_target(host, &rel), target);
    }

    #[test]
    fn next_rid_is_monotonic() {
        let rels = Relationships {
            items: vec![
                Relationship {
                    id: "rId1".into(),
                    rel_type: REL_TYPE_SLIDE_LAYOUT.into(),
                    target: "../slideLayouts/slideLayout7.xml".into(),
                    target_mode: None,
                },
                Relationship {
                    id: "rId2".into(),
                    rel_type: REL_TYPE_IMAGE.into(),
                    target: "../media/image1.png".into(),
                    target_mode: None,
                },
            ],
        };
        assert_eq!(rels.next_rid(), "rId3");
    }
}
