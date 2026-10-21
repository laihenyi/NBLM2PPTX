//! 高階門面：把 OPC 封裝、投影片定位與三個寫入器黏成呼叫端要的樣子。
//!
//! 設計上刻意讓 `Deck` 只負責「在既有 OOXML 上改」，不負責從零產生簡報。
//! 本專案的來源永遠是既有的 PPTX（Spire 產生的單頁檔，或 NotebookLM 匯出的檔），
//! 所以從範本增修比從零拼裝更安全——master / layout / theme 都不用自己造。

use std::path::Path;

use crate::background::Background;
use crate::emu::{Rect, SlideSize};
use crate::error::{Error, Result};
use crate::opc::{Package, REL_TYPE_SLIDE, REL_TYPE_SLIDE_LAYOUT};
use crate::picture::Picture;
use crate::pres::{self, PRESENTATION_PART};
use crate::shape::TextBox;
use crate::slide;
use crate::xml::Document;

/// 一份簡報。
#[derive(Debug, Clone)]
pub struct Deck {
    package: Package,
}

impl Deck {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let package = Package::open(path)?;
        if !package.contains(PRESENTATION_PART) {
            return Err(Error::PartNotFound(PRESENTATION_PART.to_owned()));
        }
        Ok(Self { package })
    }

    pub fn from_package(package: Package) -> Result<Self> {
        if !package.contains(PRESENTATION_PART) {
            return Err(Error::PartNotFound(PRESENTATION_PART.to_owned()));
        }
        Ok(Self { package })
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        self.package.save(path)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.package.to_bytes()
    }

    pub fn package(&self) -> &Package {
        &self.package
    }

    pub fn package_mut(&mut self) -> &mut Package {
        &mut self.package
    }

    pub fn into_package(self) -> Package {
        self.package
    }

    // ── 投影片定位 ──────────────────────────────────────────────────────────

    pub fn presentation(&self) -> Result<Document> {
        self.package.xml(PRESENTATION_PART)
    }

    /// 依簡報順序回傳投影片的 part 名稱。
    pub fn slide_parts(&self) -> Result<Vec<String>> {
        let pres = self.presentation()?;
        let rels = self.package.rels(PRESENTATION_PART)?;
        let mut out = Vec::new();
        for rid in pres::slide_rel_ids(&pres)? {
            let target = rels.target_of(&rid).ok_or_else(|| {
                Error::UnexpectedStructure(format!("presentation.xml.rels 缺少 {rid}"))
            })?;
            out.push(Package::resolve_target(PRESENTATION_PART, target));
        }
        Ok(out)
    }

    pub fn slide_count(&self) -> Result<usize> {
        Ok(pres::slide_count(&self.presentation()?))
    }

    /// 第 `index` 頁（0 起算）的 part 名稱。
    pub fn slide_part(&self, index: usize) -> Result<String> {
        self.slide_parts()?
            .into_iter()
            .nth(index)
            .ok_or_else(|| Error::OutOfRange(format!("投影片索引 {index} 超出範圍")))
    }

    pub fn slide(&self, index: usize) -> Result<Document> {
        let part = self.slide_part(index)?;
        self.package.xml(&part)
    }

    pub fn set_slide(&mut self, index: usize, doc: &Document) -> Result<()> {
        let part = self.slide_part(index)?;
        self.package.set_xml(&part, doc)
    }

    /// 對第 `index` 頁做就地修改。
    pub fn with_slide<R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut Document) -> Result<R>,
    ) -> Result<R> {
        let mut doc = self.slide(index)?;
        let result = f(&mut doc)?;
        self.set_slide(index, &doc)?;
        Ok(result)
    }

    /// 該頁使用的版面配置 part 名稱。
    pub fn slide_layout_part(&self, index: usize) -> Result<String> {
        let part = self.slide_part(index)?;
        let rels = self.package.rels(&part)?;
        let rel = rels
            .first_of_type(REL_TYPE_SLIDE_LAYOUT)
            .ok_or_else(|| Error::UnexpectedStructure(format!("{part} 沒有 slideLayout 關聯")))?;
        Ok(Package::resolve_target(&part, &rel.target))
    }

    // ── 畫布 ────────────────────────────────────────────────────────────────

    pub fn set_slide_size(&mut self, size: SlideSize) -> Result<()> {
        let mut pres = self.presentation()?;
        pres::set_slide_size(&mut pres, size)?;
        self.package.set_xml(PRESENTATION_PART, &pres)
    }

    pub fn apply_pdf_page_size(&mut self, width_pt: f64, height_pt: f64) -> Result<SlideSize> {
        let mut pres = self.presentation()?;
        let size = pres::apply_pdf_page_size(&mut pres, width_pt, height_pt)?;
        self.package.set_xml(PRESENTATION_PART, &pres)?;
        Ok(size)
    }

    pub fn slide_size(&self) -> Result<(i64, i64, Option<String>)> {
        pres::slide_size(&self.presentation()?)
            .ok_or_else(|| Error::NodeNotFound("p:sldSz".to_owned()))
    }

    // ── 三個寫入器的高階入口 ────────────────────────────────────────────────

    /// 在該頁末端追加一個文字框，回傳形狀 id。
    pub fn push_text_box(&mut self, index: usize, text_box: &TextBox) -> Result<u32> {
        self.with_slide(index, |doc| {
            let id = slide::next_shape_id(doc);
            slide::append_shape(doc, text_box.build(id))?;
            Ok(id)
        })
    }

    /// 在該頁最底層插入圖片（`ZOrderPosition = 0`），回傳 `(形狀 id, rId)`。
    pub fn insert_picture_at_bottom(
        &mut self,
        index: usize,
        image: Vec<u8>,
        extension: &str,
        rect: Rect,
        name: &str,
    ) -> Result<(u32, String)> {
        let part = self.slide_part(index)?;
        let rid = self.package.add_image_for(&part, image, extension)?;
        let mut doc = self.package.xml(&part)?;
        let id = slide::next_shape_id(&doc);
        let pic = Picture::new(name, rid.clone(), rect).build(id);
        slide::insert_shape_at_bottom(&mut doc, pic)?;
        self.package.set_xml(&part, &doc)?;
        Ok((id, rid))
    }

    /// 在該頁末端追加圖片（最上層）。
    pub fn push_picture(
        &mut self,
        index: usize,
        image: Vec<u8>,
        extension: &str,
        rect: Rect,
        name: &str,
    ) -> Result<(u32, String)> {
        let part = self.slide_part(index)?;
        let rid = self.package.add_image_for(&part, image, extension)?;
        let mut doc = self.package.xml(&part)?;
        let id = slide::next_shape_id(&doc);
        slide::append_shape(&mut doc, Picture::new(name, rid.clone(), rect).build(id))?;
        self.package.set_xml(&part, &doc)?;
        Ok((id, rid))
    }

    /// 把圖片設成該頁背景，回傳 rId。
    pub fn set_background_picture(
        &mut self,
        index: usize,
        image: Vec<u8>,
        extension: &str,
    ) -> Result<String> {
        let part = self.slide_part(index)?;
        let rid = self.package.add_image_for(&part, image, extension)?;
        let mut doc = self.package.xml(&part)?;
        slide::set_background(&mut doc, Background::picture(rid.clone()).build())?;
        self.package.set_xml(&part, &doc)?;
        Ok(rid)
    }

    /// 依名稱刪除形狀（處理「讀入既有 Spire 產出檔」的情境）。
    pub fn remove_shapes_named(&mut self, index: usize, name: &str) -> Result<usize> {
        self.with_slide(index, |doc| slide::remove_shapes_named(doc, name))
    }

    /// 把既有的影像 part 設成背景（不重複寫入 bytes）。
    pub fn set_background_from_media(&mut self, index: usize, media_part: &str) -> Result<String> {
        let part = self.slide_part(index)?;
        let rid = self
            .package
            .relate_existing(&part, crate::opc::REL_TYPE_IMAGE, media_part)?;
        let mut doc = self.package.xml(&part)?;
        slide::set_background(&mut doc, Background::picture(rid.clone()).build())?;
        self.package.set_xml(&part, &doc)?;
        Ok(rid)
    }

    // ── 投影片增刪 ──────────────────────────────────────────────────────────

    /// 移除第 `index` 頁，並同步清掉 `p:sldId`、簡報關聯、part 與 content-type。
    ///
    /// 被該頁獨佔的 media 不會一併刪除（PowerPoint 容忍孤兒 part，
    /// 且這些 media 可能同時被別頁或背景引用）。
    pub fn remove_slide(&mut self, index: usize) -> Result<()> {
        let part = self.slide_part(index)?;

        let mut pres_doc = self.package.xml(PRESENTATION_PART)?;
        let mut pres_rels = self.package.rels(PRESENTATION_PART)?;
        let rid = pres_rels
            .items
            .iter()
            .find(|r| Package::resolve_target(PRESENTATION_PART, &r.target) == part)
            .map(|r| r.id.clone());

        if let Some(rid) = rid {
            pres::remove_slide_rel(&mut pres_doc, &rid);
            pres_rels.items.retain(|r| r.id != rid);
            self.package.set_rels(PRESENTATION_PART, &pres_rels)?;
            self.package.set_xml(PRESENTATION_PART, &pres_doc)?;
        }

        self.package.remove_part(&part);
        self.package.remove_part(&Package::rels_part_name(&part));
        self.package.remove_override_content_type(&part)?;
        Ok(())
    }

    /// 只保留第 `keep` 頁，其餘刪除。
    pub fn keep_only_slide(&mut self, keep: usize) -> Result<()> {
        let count = self.slide_count()?;
        if keep >= count {
            return Err(Error::OutOfRange(format!(
                "投影片索引 {keep} 超出範圍（共 {count} 頁）"
            )));
        }
        for index in (0..count).rev() {
            if index != keep {
                self.remove_slide(index)?;
            }
        }
        Ok(())
    }

    // ── refine_ppt 需要的操作 ───────────────────────────────────────────────

    /// 統一該頁所有文字的字型，回傳改寫的 run 數量。
    pub fn set_all_fonts(&mut self, index: usize, typeface: &str) -> Result<usize> {
        self.with_slide(index, |doc| Ok(slide::set_all_fonts(doc, typeface)))
    }

    /// 移除該頁所有非文字框形狀（`p:pic` / `p:graphicFrame` / `p:grpSp` / `p:cxnSp`）。
    pub fn remove_non_text_shapes(&mut self, index: usize) -> Result<usize> {
        self.with_slide(index, slide::remove_non_text_shapes)
    }

    /// 讀取該頁既有背景圖的原始位元組；沒有背景或不是圖片填滿時回 `None`。
    pub fn background_image(&self, index: usize) -> Result<Option<Vec<u8>>> {
        let part = self.slide_part(index)?;
        let doc = self.package.xml(&part)?;
        let Some(rid) = slide::background_blip_rid(&doc) else {
            return Ok(None);
        };
        let rels = self.package.rels(&part)?;
        let Some(target) = rels.target_of(rid) else {
            return Ok(None);
        };
        let media_part = Package::resolve_target(&part, target);
        Ok(Some(self.package.part(&media_part)?.to_vec()))
    }

    /// 該頁所有文字框的位置與內容摘要。
    pub fn text_boxes(&self, index: usize) -> Result<Vec<slide::TextBoxInfo>> {
        slide::text_shapes(&self.slide(index)?)
    }

    // ── 檔案屬性 ────────────────────────────────────────────────────────────

    /// 修正 `docProps/app.xml`（`PresentationFormat` 與 `Slides`）。
    pub fn sync_app_properties(&mut self) -> Result<()> {
        crate::props::sync(&mut self.package)
    }

    /// 合併多份來源簡報（取代 Spire 的 `Slides.AppendBySlide`）。
    ///
    /// 基底只保留第一頁，之後每份來源取第一頁附加。詳見 [`crate::merge`]。
    pub fn merge(
        &mut self,
        sources: &[Deck],
        options: &crate::merge::MergeOptions,
    ) -> Result<crate::merge::MergeReport> {
        crate::merge::merge_sources(self, sources, options.clone())
    }

    // ── 合併用的一次性查詢 ──────────────────────────────────────────────────

    /// 該頁的版面配置 XML 內容（用於判斷兩份簡報是否同構）。
    pub fn slide_layout_bytes(&self, index: usize) -> Result<Vec<u8>> {
        let part = self.slide_layout_part(index)?;
        Ok(self.package.part(&part)?.to_vec())
    }

    /// 簡報層級的投影片關聯型別常數檢查（避免呼叫端硬寫字串）。
    pub fn slide_rel_type() -> &'static str {
        REL_TYPE_SLIDE
    }
}
