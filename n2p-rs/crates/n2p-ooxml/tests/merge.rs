//! 合併的 golden tests。
//!
//! 基準：`spire.presentation` 11.8.1 的 `Slides.AppendBySlide`（產出見
//! `rust-rewrite/assets/probe-pptx/4_merged.pptx`）。我們不重現它的膨脹行為，
//! 而是驗證「設計共用」這一條路徑正確，而且比它小。

use std::path::PathBuf;

use n2p_ooxml::merge::{self, MergeOptions};
use n2p_ooxml::{Deck, Package, xml};

fn assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../rust-rewrite/assets/probe-pptx")
}

fn probe(name: &str) -> PathBuf {
    let path = assets_dir().join(name);
    assert!(path.exists(), "缺少基準檔 {path:?}");
    path
}

fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("n2p-ooxml-tests");
    std::fs::create_dir_all(&dir).expect("建立暫存目錄");
    dir.join(name)
}

fn page_texts(deck: &Deck) -> Vec<Vec<String>> {
    (0..deck.slide_count().unwrap())
        .map(|index| {
            deck.text_boxes(index)
                .unwrap()
                .into_iter()
                .map(|tb| tb.text)
                .filter(|t| !t.is_empty())
                .collect()
        })
        .collect()
}

// ── 1. 設計共用：3 份同模板 → 1 套 master/layout/theme ─────────────────────

#[test]
fn merge_reuses_one_design_set_instead_of_spires_three() {
    let mut base = Deck::open(probe("src_a.pptx")).unwrap();
    let sources = vec![
        Deck::open(probe("src_b.pptx")).unwrap(),
        Deck::open(probe("src_c.pptx")).unwrap(),
    ];

    let base_design = merge::design_part_counts(&base);
    let report = base.merge(&sources, &MergeOptions::default()).unwrap();

    assert_eq!(report.slides, 3, "應為 3 頁");
    assert_eq!(report.reused_design_sets, 2, "兩份來源都應走同構捷徑");
    assert_eq!(report.imported_design_sets, 0, "不應觸發設計組匯入");
    assert!(report.is_fully_deduplicated());

    // 設計組數量不變 —— 這就是相對 Spire 的關鍵改善
    assert_eq!(
        merge::design_part_counts(&base),
        base_design,
        "設計組不應增加"
    );

    // 對照 Spire 的盲目匯入：3 套 master、33 個 layout、3 個 theme
    let spire = Deck::open(probe("4_merged.pptx")).unwrap();
    assert_eq!(
        merge::design_part_counts(&spire),
        (3, 33, 3),
        "基準檔應為 Spire 的膨脹結果"
    );

    let ours = temp_path("merged_dedup.pptx");
    base.save(&ours).unwrap();
    let ours_size = std::fs::metadata(&ours).unwrap().len();
    let spire_size = std::fs::metadata(probe("4_merged.pptx")).unwrap().len();
    assert!(
        ours_size < spire_size,
        "去重後應比 Spire 小（實際 {ours_size} vs {spire_size}）"
    );
    println!(
        "去重後 {ours_size} bytes，Spire {spire_size} bytes（{:.0}%）",
        ours_size as f64 / spire_size as f64 * 100.0
    );
}

#[test]
fn merged_slides_keep_their_content_and_order() {
    let mut base = Deck::open(probe("src_a.pptx")).unwrap();
    let sources = vec![
        Deck::open(probe("src_b.pptx")).unwrap(),
        Deck::open(probe("src_c.pptx")).unwrap(),
    ];
    base.merge(&sources, &MergeOptions::default()).unwrap();

    // 每份來源第一頁的文字，應依序出現在合併後的第 1..3 頁
    let expected: Vec<String> = [
        probe("src_a.pptx"),
        probe("src_b.pptx"),
        probe("src_c.pptx"),
    ]
    .iter()
    .map(|path| {
        let deck = Deck::open(path).unwrap();
        deck.text_boxes(0)
            .unwrap()
            .into_iter()
            .map(|tb| tb.text)
            .find(|t| !t.is_empty())
            .unwrap()
    })
    .collect();
    let actual = page_texts(&base);
    for (index, want) in expected.iter().enumerate() {
        assert!(
            actual[index].iter().any(|t| t == want),
            "第 {index} 頁應含 {want:?}，實際 {actual:?}"
        );
    }
    assert_eq!(base.slide_count().unwrap(), 3);

    // 每頁的 sldId 皆唯一且 ≥ 256
    let ids: Vec<u32> = (0..3)
        .map(|i| merge::slide_id_of(&base, i).unwrap().unwrap())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 3, "sldId 應唯一");
    assert!(
        ids.iter().all(|id| *id >= 256),
        "sldId 應 ≥ 256，實際 {ids:?}"
    );
}

#[test]
fn merged_file_is_structurally_valid_and_reopens() {
    let mut base = Deck::open(probe("src_a.pptx")).unwrap();
    let sources = vec![Deck::open(probe("src_b.pptx")).unwrap()];
    base.merge(&sources, &MergeOptions::default()).unwrap();

    let out = temp_path("merged_reopen.pptx");
    base.save(&out).unwrap();

    // 重新開啟並逐層檢查自洽性
    let reopened = Deck::open(&out).unwrap();
    assert_eq!(reopened.slide_count().unwrap(), 2);

    let pres_doc = reopened.presentation().unwrap();
    let pres_rels = reopened
        .package()
        .rels(n2p_ooxml::pres::PRESENTATION_PART)
        .unwrap();
    for rid in n2p_ooxml::pres::slide_rel_ids(&pres_doc).unwrap() {
        let target = pres_rels
            .target_of(&rid)
            .unwrap_or_else(|| panic!("{rid} 沒有對應關聯"));
        let part = Package::resolve_target(n2p_ooxml::pres::PRESENTATION_PART, target);
        assert!(reopened.package().contains(&part), "{part} 不存在");
        assert!(
            reopened.package().contains(&Package::rels_part_name(&part)),
            "{part} 缺 .rels"
        );
    }

    // Content_Types 需為每一頁註冊 Override
    let ct = reopened.package().xml("[Content_Types].xml").unwrap();
    let overrides: Vec<&str> = ct
        .root
        .elements("Override")
        .filter_map(|e| e.get_attr("PartName"))
        .collect();
    for part in reopened.slide_parts().unwrap() {
        let key = format!("/{part}");
        assert!(
            overrides.contains(&key.as_str()),
            "Content_Types 缺少 {key}"
        );
    }

    // 每頁都能解析出 spTree
    for index in 0..2 {
        let doc = reopened.slide(index).unwrap();
        assert!(doc.root.path(&["p:sld", "p:cSld", "p:spTree"]).is_some());
    }
}

#[test]
fn merge_shares_the_base_layout_rather_than_importing() {
    let mut base = Deck::open(probe("src_a.pptx")).unwrap();
    let base_layout = base.slide_layout_part(0).unwrap();
    base.merge(
        &[Deck::open(probe("src_b.pptx")).unwrap()],
        &MergeOptions::default(),
    )
    .unwrap();

    for index in 0..2 {
        assert_eq!(
            base.slide_layout_part(index).unwrap(),
            base_layout,
            "第 {index} 頁應指向基底同一份 layout"
        );
    }
}

// ── 2. 非同構來源：退回設計組匯入 ──────────────────────────────────────────

/// 造一份「版面配置被改過」的來源，用來觸發 fallback。
fn make_non_isomorphic_source() -> PathBuf {
    let mut deck = Deck::open(probe("src_b.pptx")).unwrap();
    let layout_part = deck.slide_layout_part(0).unwrap();
    let bytes = deck.package().part(&layout_part).unwrap().to_vec();
    let mut doc = xml::parse(&bytes).unwrap();
    // 動一個無害屬性，讓 layout 的位元組內容與基底不同
    doc.root.set_attr("showMasterSp", "0");
    let mutated = xml::to_bytes(&doc).unwrap();
    assert_ne!(mutated, bytes);
    deck.package_mut().set_part(layout_part, mutated);

    let out = temp_path("non_isomorphic.pptx");
    deck.save(&out).unwrap();
    out
}

#[test]
fn non_isomorphic_source_falls_back_to_design_import() {
    let source = Deck::open(make_non_isomorphic_source()).unwrap();
    let mut base = Deck::open(probe("src_a.pptx")).unwrap();
    let before = merge::design_part_counts(&base);

    let report = base.merge(&[source], &MergeOptions::default()).unwrap();
    assert_eq!(report.imported_design_sets, 1, "應走設計組匯入");
    assert_eq!(report.reused_design_sets, 0);

    let after = merge::design_part_counts(&base);
    assert_eq!(after.0, before.0 + 1, "應多一個 master");
    assert_eq!(after.1, before.1 + 11, "應多一整套 11 個 layout");
    assert_eq!(after.2, before.2 + 1, "應多一個 theme");

    // 兩頁各自指向自己的 layout，且都指向存在的 part
    let layout_0 = base.slide_layout_part(0).unwrap();
    let layout_1 = base.slide_layout_part(1).unwrap();
    assert_ne!(layout_0, layout_1);
    assert!(base.package().contains(&layout_0));
    assert!(base.package().contains(&layout_1));

    // 匯入的 master 必須有 sldMasterId 與簡報關聯
    let pres_doc = base.presentation().unwrap();
    let masters = pres_doc.root.child_el("p:sldMasterIdLst").unwrap();
    assert_eq!(masters.elements("p:sldMasterId").count(), 2);

    let out = temp_path("non_isomorphic_merged.pptx");
    base.save(&out).unwrap();
    let reopened = Deck::open(&out).unwrap();
    assert_eq!(reopened.slide_count().unwrap(), 2);
}

#[test]
fn disallowing_design_import_reports_the_mismatch() {
    let source = Deck::open(make_non_isomorphic_source()).unwrap();
    let mut base = Deck::open(probe("src_a.pptx")).unwrap();
    let options = MergeOptions {
        allow_design_import: false,
        sync_app_properties: false,
    };

    let error = base.merge(&[source], &options).expect_err("應報錯");
    let message = error.to_string();
    assert!(
        message.contains("版面配置與基底不同"),
        "錯誤訊息應指出原因，實際：{message}"
    );
}

// ── 3. docProps 修正 ────────────────────────────────────────────────────────

#[test]
fn merge_fixes_slide_count_and_presentation_format() {
    let mut base = Deck::open(probe("src_a.pptx")).unwrap();

    // Spire 的 4_merged.pptx：3 頁，但 app.xml 仍寫 Slides=1
    let spire = Deck::open(probe("4_merged.pptx")).unwrap();
    assert_eq!(
        n2p_ooxml::props::slides_of(spire.package()).unwrap(),
        Some(1),
        "基準檔應重現 Spire 的 bug"
    );
    assert_eq!(
        n2p_ooxml::props::presentation_format_of(spire.package())
            .unwrap()
            .as_deref(),
        Some("全屏显示(4:3)"),
        "基準檔應重現 Spire 的 16:9 誤標"
    );

    let sources = vec![
        Deck::open(probe("src_b.pptx")).unwrap(),
        Deck::open(probe("src_c.pptx")).unwrap(),
    ];
    base.merge(&sources, &MergeOptions::default()).unwrap();

    assert_eq!(
        n2p_ooxml::props::slides_of(base.package()).unwrap(),
        Some(3),
        "Slides 應為實際頁數"
    );
    assert_eq!(
        n2p_ooxml::props::presentation_format_of(base.package())
            .unwrap()
            .as_deref(),
        Some("Widescreen"),
        "16:9 應寫 Widescreen"
    );
}

#[test]
fn app_properties_sync_respects_custom_sizes() {
    let mut deck = Deck::open(probe("sz_4x3.pptx")).unwrap();
    deck.set_slide_size(n2p_ooxml::SlideSize::Screen16x10)
        .unwrap();
    deck.sync_app_properties().unwrap();
    assert_eq!(
        n2p_ooxml::props::presentation_format_of(deck.package())
            .unwrap()
            .as_deref(),
        Some("On-screen Show (16:10)")
    );

    deck.set_slide_size(n2p_ooxml::SlideSize::Custom {
        cx: 1_000_000,
        cy: 2_000_000,
    })
    .unwrap();
    deck.sync_app_properties().unwrap();
    assert_eq!(
        n2p_ooxml::props::presentation_format_of(deck.package())
            .unwrap()
            .as_deref(),
        Some("Custom")
    );
}

// ── 4. 邊界情況 ────────────────────────────────────────────────────────────

#[test]
fn merging_into_a_multi_slide_base_keeps_only_the_first() {
    let mut base = Deck::open(probe("4_merged.pptx")).unwrap();
    assert_eq!(base.slide_count().unwrap(), 3);
    let report = base.merge(&[], &MergeOptions::default()).unwrap();
    assert_eq!(report.slides, 1);
    assert_eq!(base.slide_count().unwrap(), 1, "基底應只保留第一頁");
}

#[test]
fn remove_slide_cleans_parts_and_content_types() {
    let mut deck = Deck::open(probe("4_merged.pptx")).unwrap();
    let victim = deck.slide_part(1).unwrap();
    deck.remove_slide(1).unwrap();

    assert_eq!(deck.slide_count().unwrap(), 2);
    assert!(!deck.package().contains(&victim), "slide part 應被刪除");
    assert!(
        !deck.package().contains(&Package::rels_part_name(&victim)),
        "rels 應被刪除"
    );

    let ct = deck.package().xml("[Content_Types].xml").unwrap();
    assert!(
        !ct.root
            .elements("Override")
            .any(|e| e.get_attr("PartName") == Some(format!("/{victim}").as_str())),
        "Content_Types 應移除該頁"
    );

    // 剩下的兩頁仍可正常讀取
    for index in 0..2 {
        assert!(
            deck.slide(index)
                .unwrap()
                .root
                .path(&["p:sld", "p:cSld", "p:spTree"])
                .is_some()
        );
    }
}

#[test]
fn keep_only_slide_works_from_any_index() {
    let mut deck = Deck::open(probe("4_merged.pptx")).unwrap();
    let want = deck.slide(2).unwrap();
    let want_text: Vec<String> = n2p_ooxml::slide::text_shapes(&want)
        .unwrap()
        .into_iter()
        .map(|t| t.text)
        .collect();

    deck.keep_only_slide(2).unwrap();
    assert_eq!(deck.slide_count().unwrap(), 1);
    let got: Vec<String> = deck
        .text_boxes(0)
        .unwrap()
        .into_iter()
        .map(|t| t.text)
        .collect();
    assert_eq!(got, want_text);
}

#[test]
fn merge_files_helper_produces_a_readable_deck() {
    let out = temp_path("merged_from_files.pptx");
    let report = merge::merge_files(
        &[
            probe("src_a.pptx"),
            probe("src_b.pptx"),
            probe("src_c.pptx"),
        ],
        &out,
        MergeOptions::default(),
    )
    .unwrap();

    assert_eq!(report.slides, 3);
    let deck = Deck::open(&out).unwrap();
    assert_eq!(deck.slide_count().unwrap(), 3);
    assert_eq!(merge::design_part_counts(&deck).0, 1, "應只有一個 master");
}
