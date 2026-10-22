//! 合併多份單頁簡報，取代 Spire 的 `Slides.AppendBySlide`。
//!
//! ## 為什麼要自己寫
//!
//! 實測 Spire.Presentation 11.8.1 的 `AppendBySlide` 是**盲目 part 匯入**：
//! 3 份同模板單頁 PPTX 合併後，產出 3 個 `slideMaster`、33 個 `slideLayout`、
//! 3 個 `theme`，檔案 74,687 bytes。它不判斷來源設計是否等價。
//!
//! 本模組預設走「同構捷徑」：先用 layout 的位元組內容判斷來源是否同一套設計，
//! 是就只搬 `slideN.xml` 與它引用到的 media，layout/master/theme 全部共用基底那一套。
//! 依 `tests/golden.rs` 的實測，同模板來源的 layout 內容逐位元相同，因此去重是安全的。
//!
//! 判斷失敗（來源真的是不同模板）時才退回完整設計組匯入 —— 也就是 Spire 的行為。

use std::collections::HashMap;

use crate::deck::Deck;
use crate::error::{Error, Result};
use crate::opc::{
    CT_SLIDE, CT_SLIDE_LAYOUT, CT_SLIDE_MASTER, CT_THEME, REL_TYPE_IMAGE, REL_TYPE_SLIDE,
    REL_TYPE_SLIDE_LAYOUT, REL_TYPE_SLIDE_MASTER, Relationship, Relationships, image_content_type,
};
use crate::pres::{self, PRESENTATION_PART};
use crate::slide;
use crate::xml::{self, Document};

/// 合併選項。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeOptions {
    /// 是否允許在來源設計不同時匯入整套設計組。
    ///
    /// 關掉之後，遇到非同構來源會直接報錯，適合「我要確定所有輸入都同模板」的情境。
    pub allow_design_import: bool,
    /// 合併後是否順手修正 `docProps/app.xml`（`Slides` 頁數等）。
    pub sync_app_properties: bool,
}

impl Default for MergeOptions {
    fn default() -> Self {
        Self {
            allow_design_import: true,
            sync_app_properties: true,
        }
    }
}

/// 合併結果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// 合併後的總頁數。
    pub slides: usize,
    /// 直接沿用基底設計的來源份數（同構捷徑命中）。
    pub reused_design_sets: usize,
    /// 需要匯入整套設計組的來源份數（非同構，退回 Spire 行為）。
    pub imported_design_sets: usize,
    /// 從來源搬過來的 media part 數量。
    pub copied_media: usize,
}

impl MergeReport {
    /// 是否全部走同構捷徑。
    pub fn is_fully_deduplicated(&self) -> bool {
        self.imported_design_sets == 0
    }
}

/// 把 `sources` 的第一頁依序附加到 `base`。
///
/// `base` 只保留第一頁，行為與 `combine_ppt_files_with_spire` 一致。
pub fn merge_sources(
    base: &mut Deck,
    sources: &[Deck],
    options: MergeOptions,
) -> Result<MergeReport> {
    base.keep_only_slide(0)?;

    let mut layout_index = build_layout_index(base)?;
    let mut report = MergeReport {
        slides: 1,
        ..Default::default()
    };

    for source in sources {
        let outcome = append_one_slide(base, source, 0, &mut layout_index, &options)?;
        if outcome.reused_design {
            report.reused_design_sets += 1;
        } else {
            report.imported_design_sets += 1;
        }
        report.copied_media += outcome.copied_media;
        report.slides += 1;
    }

    if options.sync_app_properties {
        base.sync_app_properties()?;
    }
    Ok(report)
}

/// 單次附加的結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppendOutcome {
    /// 是否沿用基底既有的設計組（同構捷徑命中）。
    pub reused_design: bool,
    /// 從來源搬過來的 media part 數量。
    pub copied_media: usize,
}

/// 把 `source` 的第 `source_index` 頁附加到 `base` 末端。
///
/// 這是合併的基本單位，也讓「從空白範本逐頁長出簡報」可以重用同一段邏輯。
pub fn append_slide(base: &mut Deck, source: &Deck, source_index: usize) -> Result<AppendOutcome> {
    let mut layout_index = build_layout_index(base)?;
    append_one_slide(
        base,
        source,
        source_index,
        &mut layout_index,
        &MergeOptions::default(),
    )
}

/// 建立「layout 內容 → part 名稱」索引，用於同構判斷。
fn build_layout_index(deck: &Deck) -> Result<HashMap<Vec<u8>, String>> {
    let mut index = HashMap::new();
    for name in deck.package().part_names() {
        if name.starts_with("ppt/slideLayouts/")
            && name.ends_with(".xml")
            && !name.contains("/_rels/")
        {
            index.insert(deck.package().part(name)?.to_vec(), name.to_owned());
        }
    }
    Ok(index)
}

fn append_one_slide(
    base: &mut Deck,
    source: &Deck,
    source_index: usize,
    layout_index: &mut HashMap<Vec<u8>, String>,
    options: &MergeOptions,
) -> Result<AppendOutcome> {
    if source.slide_count()? <= source_index {
        return Err(Error::UnexpectedStructure(format!(
            "來源簡報沒有第 {source_index} 頁（共 {} 頁）",
            source.slide_count()?
        )));
    }

    let src_part = source.slide_part(source_index)?;
    let src_doc = source.slide(source_index)?;
    let src_rels = source.package().rels(&src_part)?;
    let src_layout_part = source.slide_layout_part(source_index)?;
    let src_layout_bytes = source.package().part(&src_layout_part)?.to_vec();
    let src_layout_rid = src_rels
        .items
        .iter()
        .find(|r| r.rel_type == REL_TYPE_SLIDE_LAYOUT)
        .map(|r| r.id.clone());

    // ── 決定目標 layout ────────────────────────────────────────────────────
    let (target_layout, reused_design) = match layout_index.get(&src_layout_bytes) {
        Some(existing) => (existing.clone(), true),
        None if options.allow_design_import => {
            let import = adopt_design_set(base, source)?;
            for (bytes, name) in &import.layout_bytes {
                layout_index.insert(bytes.clone(), name.clone());
            }
            let target = import
                .part_map
                .get(&src_layout_part)
                .cloned()
                .ok_or_else(|| {
                    Error::UnexpectedStructure(format!("匯入設計組後找不到 {src_layout_part}"))
                })?;
            (target, false)
        }
        None => {
            return Err(Error::UnexpectedStructure(format!(
                "來源 {src_part} 的版面配置與基底不同，且已停用設計組匯入"
            )));
        }
    };

    // ── 建立新頁的關聯表 ───────────────────────────────────────────────────
    let new_slide_part = base.package().alloc_slide_part();
    let mut new_rels = Relationships::default();
    let mut mapping: HashMap<String, String> = HashMap::new();
    let mut copied_media = 0usize;

    let layout_rid = new_rels.next_rid();
    new_rels.push(Relationship {
        id: layout_rid.clone(),
        rel_type: REL_TYPE_SLIDE_LAYOUT.to_owned(),
        target: crate::opc::Package::relative_target(&new_slide_part, &target_layout),
        target_mode: None,
    });
    if let Some(rid) = src_layout_rid {
        mapping.insert(rid, layout_rid);
    }

    for rel in &src_rels.items {
        if rel.rel_type == REL_TYPE_SLIDE_LAYOUT {
            continue;
        }
        if rel.rel_type == REL_TYPE_IMAGE {
            let media_part = crate::opc::Package::resolve_target(&src_part, &rel.target);
            let extension = media_part
                .rsplit('.')
                .next()
                .unwrap_or("png")
                .to_ascii_lowercase();
            let content_type = image_content_type(&extension).ok_or_else(|| {
                Error::OutOfRange(format!("來源 media part 副檔名無法辨識：{media_part}"))
            })?;

            let new_media = base.package().alloc_media_part(&extension);
            base.package_mut().set_part(
                new_media.clone(),
                source.package().part(&media_part)?.to_vec(),
            );
            base.package_mut()
                .ensure_default_content_type(&extension, content_type)?;

            let rid = new_rels.next_rid();
            new_rels.push(Relationship {
                id: rid.clone(),
                rel_type: REL_TYPE_IMAGE.to_owned(),
                target: crate::opc::Package::relative_target(&new_slide_part, &new_media),
                target_mode: None,
            });
            mapping.insert(rel.id.clone(), rid);
            copied_media += 1;
            continue;
        }
        // 其他型別（備忘稿、標籤、超連結…）在單頁截圖流程中不存在；
        // 直接略過，避免搬入無用的 part。
    }

    // ── 寫入新頁 ───────────────────────────────────────────────────────────
    let mut new_doc = src_doc;
    slide::rewrite_rel_ids(&mut new_doc, &mapping);

    base.package_mut().set_xml(&new_slide_part, &new_doc)?;
    base.package_mut().set_rels(&new_slide_part, &new_rels)?;
    base.package_mut()
        .ensure_override_content_type(&new_slide_part, CT_SLIDE)?;

    // ── 登記到 p:sldIdLst ──────────────────────────────────────────────────
    let mut pres_doc = base.presentation()?;
    let mut pres_rels = base.package().rels(PRESENTATION_PART)?;
    let rid = pres_rels.next_rid();
    pres_rels.push(Relationship {
        id: rid.clone(),
        rel_type: REL_TYPE_SLIDE.to_owned(),
        target: crate::opc::Package::relative_target(PRESENTATION_PART, &new_slide_part),
        target_mode: None,
    });
    pres::push_slide_rel(&mut pres_doc, &rid)?;
    base.package_mut().set_rels(PRESENTATION_PART, &pres_rels)?;
    base.package_mut().set_xml(PRESENTATION_PART, &pres_doc)?;

    Ok(AppendOutcome {
        reused_design,
        copied_media,
    })
}

/// 把來源的一整套設計（layouts + masters + themes）匯入基底，回傳新舊 part 對照。
struct DesignImport {
    part_map: HashMap<String, String>,
    layout_bytes: Vec<(Vec<u8>, String)>,
}

/// 設計組所在的三個目錄：(part 目錄, 檔名前綴, content type)。
const DESIGN_DIRS: [(&str, &str, &str); 3] = [
    ("ppt/slideLayouts", "slideLayout", CT_SLIDE_LAYOUT),
    ("ppt/slideMasters", "slideMaster", CT_SLIDE_MASTER),
    ("ppt/theme", "theme", CT_THEME),
];

fn adopt_design_set(base: &mut Deck, source: &Deck) -> Result<DesignImport> {
    // 第一階段：配置新名稱並立刻寫入內容。
    //
    // 必須「配置即寫入」，不能先把所有名稱算完再寫 —— 否則
    // `alloc_numbered_part` 每次都看到同樣的基底狀態，會回傳同一個名字。
    let mut part_map: HashMap<String, String> = HashMap::new();
    let mut ordered: Vec<(String, String, &str)> = Vec::new(); // (舊名, 新名, CT)

    for (dir, prefix, content_type) in DESIGN_DIRS {
        let mut old_names: Vec<String> = source
            .package()
            .part_names()
            .filter(|n| {
                n.starts_with(&format!("{dir}/")) && n.ends_with(".xml") && !n.contains("/_rels/")
            })
            .map(str::to_owned)
            .collect();
        old_names.sort();
        for old in old_names {
            let new = base.package().alloc_numbered_part(dir, prefix, "xml");
            part_map.insert(old.clone(), new.clone());
            base.package_mut()
                .set_part(new.clone(), source.package().part(&old)?.to_vec());
            base.package_mut()
                .ensure_override_content_type(&new, content_type)?;
            ordered.push((old, new, content_type));
        }
    }

    // 第二階段：複製 rels，並把 Target 改寫成新名稱。
    for (old_part, new_part, _) in &ordered {
        let old_rels_name = crate::opc::Package::rels_part_name(old_part);
        if !source.package().contains(&old_rels_name) {
            continue;
        }
        let mut rels_doc = source.package().xml(&old_rels_name)?;

        // rels 指向的 media（master 可能帶背景圖）需要一併複製。
        let mut media_map: HashMap<String, String> = HashMap::new();
        let mut needed_media: Vec<String> = Vec::new();
        for rel in rels_doc.root.elements("Relationship") {
            let Some(target) = rel.get_attr("Target") else {
                continue;
            };
            let resolved = crate::opc::Package::resolve_target(old_part, target);
            if part_map.contains_key(&resolved) || media_map.contains_key(&resolved) {
                continue;
            }
            if resolved.starts_with("ppt/media/") && source.package().contains(&resolved) {
                needed_media.push(resolved);
            }
        }

        for old_media in needed_media {
            let extension = old_media
                .rsplit('.')
                .next()
                .unwrap_or("png")
                .to_ascii_lowercase();
            let Some(content_type) = image_content_type(&extension) else {
                continue;
            };
            let new_media = base.package().alloc_media_part(&extension);
            base.package_mut().set_part(
                new_media.clone(),
                source.package().part(&old_media)?.to_vec(),
            );
            base.package_mut()
                .ensure_default_content_type(&extension, content_type)?;
            media_map.insert(old_media, new_media);
        }

        xml::rewrite_attributes(&mut rels_doc.root, &mut |key, value| {
            if key != "Target" {
                return;
            }
            let resolved = crate::opc::Package::resolve_target(old_part, value);
            if let Some(mapped) = part_map.get(&resolved).or_else(|| media_map.get(&resolved)) {
                *value = crate::opc::Package::relative_target(new_part, mapped);
            }
        });

        base.package_mut()
            .set_xml(&crate::opc::Package::rels_part_name(new_part), &rels_doc)?;
    }

    // 第三階段：為每個匯入的 master 登記 p:sldMasterId 與簡報層關聯。
    let new_masters: Vec<String> = ordered
        .iter()
        .filter(|(_, _, ct)| *ct == CT_SLIDE_MASTER)
        .map(|(_, new, _)| new.clone())
        .collect();

    if !new_masters.is_empty() {
        let mut pres_doc = base.presentation()?;
        let mut pres_rels = base.package().rels(PRESENTATION_PART)?;
        for master in &new_masters {
            let rid = pres_rels.next_rid();
            pres_rels.push(Relationship {
                id: rid.clone(),
                rel_type: REL_TYPE_SLIDE_MASTER.to_owned(),
                target: crate::opc::Package::relative_target(PRESENTATION_PART, master),
                target_mode: None,
            });
            pres::add_master_id(&mut pres_doc, &rid)?;
        }
        base.package_mut().set_rels(PRESENTATION_PART, &pres_rels)?;
        base.package_mut().set_xml(PRESENTATION_PART, &pres_doc)?;
    }

    let layout_bytes = ordered
        .iter()
        .filter(|(_, _, ct)| *ct == CT_SLIDE_LAYOUT)
        .map(|(_, new, _)| {
            base.package()
                .part(new)
                .map(|bytes| (bytes.to_vec(), new.clone()))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(DesignImport {
        part_map,
        layout_bytes,
    })
}

/// 便利函式：直接從檔案路徑合併。第一個檔案當基底。
pub fn merge_files(
    paths: &[std::path::PathBuf],
    out: &std::path::Path,
    options: MergeOptions,
) -> Result<MergeReport> {
    let mut decks = Vec::new();
    for path in paths {
        decks.push(Deck::open(path)?);
    }
    let mut iter = decks.into_iter();
    let mut base = iter
        .next()
        .ok_or_else(|| Error::OutOfRange("至少要提供一個輸入檔".into()))?;
    let sources: Vec<Deck> = iter.collect();
    let report = merge_sources(&mut base, &sources, options)?;
    base.save(out)?;
    Ok(report)
}

/// 供測試與診斷：列出簡報目前用到的 master / layout / theme part 數量。
pub fn design_part_counts(deck: &Deck) -> (usize, usize, usize) {
    let count = |dir: &str| {
        deck.package()
            .part_names()
            .filter(|n| {
                n.starts_with(&format!("{dir}/")) && n.ends_with(".xml") && !n.contains("/_rels/")
            })
            .count()
    };
    (
        count("ppt/slideMasters"),
        count("ppt/slideLayouts"),
        count("ppt/theme"),
    )
}

/// 供測試用：取得某頁的 `p:sldId/@id`。
pub fn slide_id_of(deck: &Deck, index: usize) -> Result<Option<u32>> {
    let pres_doc: Document = deck.presentation()?;
    let list = pres_doc
        .root
        .child_el("p:sldIdLst")
        .ok_or_else(|| Error::NodeNotFound("p:sldIdLst".to_owned()))?;
    Ok(list
        .elements("p:sldId")
        .nth(index)
        .and_then(|e| e.get_attr("id"))
        .and_then(|v| v.parse().ok()))
}
