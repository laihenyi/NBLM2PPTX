//! Golden tests：以 `spire.presentation` 11.8.1 的實際輸出為基準，
//! 驗證 `n2p-ooxml` 寫出的節點結構與它一致。
//!
//! 基準檔位於 `<repo>/rust-rewrite/assets/probe-pptx/`，產生方式見
//! `rust-rewrite/README.md`。若基準檔不存在，測試會明確失敗而非靜默跳過——
//! 這是刻意的：這批檔案就是規格，缺少它們代表驗證沒有發生。

use std::path::PathBuf;

use n2p_ooxml::opc::{Package, REL_TYPE_IMAGE};
use n2p_ooxml::xml::{Element, Node};
use n2p_ooxml::{Background, Deck, Picture, Rect, SlideSize, TextBox, xml};

fn assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../rust-rewrite/assets/probe-pptx")
}

fn probe(name: &str) -> PathBuf {
    let path = assets_dir().join(name);
    assert!(
        path.exists(),
        "缺少基準檔 {path:?}；請依 rust-rewrite/README.md 重新產生 probe-pptx"
    );
    path
}

fn slide_part_name(deck: &Deck) -> String {
    deck.slide_part(0).expect("取得第 0 頁 part")
}

fn sp_tree(doc: &xml::Document) -> &Element {
    doc.root
        .path(&["p:sld", "p:cSld", "p:spTree"])
        .expect("p:sld/p:cSld/p:spTree")
}

fn find_sp<'a>(doc: &'a xml::Document, name: &'a str) -> &'a Element {
    sp_tree(doc)
        .elements("p:sp")
        .find(|sp| {
            sp.path(&["p:nvSpPr", "p:cNvPr"])
                .and_then(|e| e.get_attr("name"))
                == Some(name)
        })
        .unwrap_or_else(|| panic!("找不到名為 {name} 的 p:sp"))
}

fn find_pic(doc: &xml::Document) -> &Element {
    sp_tree(doc).elements("p:pic").next().expect("找不到 p:pic")
}

fn assert_same_structure(actual: &Element, expected: &Element, context: &str) {
    let actual = xml::normalize_for_compare(actual);
    let expected = xml::normalize_for_compare(expected);
    assert_eq!(
        describe(&actual),
        describe(&expected),
        "{context}：結構不符\n實際：{actual:#?}\n預期：{expected:#?}"
    );
    assert_eq!(actual, expected, "{context}：結構不符");
}

/// 把元素攤平成可讀字串，讓失敗訊息指出第一個不同的節點。
fn describe(el: &Element) -> String {
    let mut out = String::new();
    fn walk(el: &Element, depth: usize, out: &mut String) {
        let attrs = el
            .attrs
            .iter()
            .filter(|(k, _)| !k.starts_with("xmlns"))
            .map(|(k, v)| format!("{k}={v:?}"))
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!("{}{} {}\n", "  ".repeat(depth), el.name, attrs));
        for child in &el.children {
            match child {
                Node::Element(e) => walk(e, depth + 1, out),
                Node::Text(t) => out.push_str(&format!("{}#text {t:?}\n", "  ".repeat(depth + 1))),
                Node::Comment(c) => {
                    out.push_str(&format!("{}!comment {c:?}\n", "  ".repeat(depth + 1)))
                }
            }
        }
    }
    walk(el, 0, &mut out);
    out
}

// ── 1. OPC + XML 往返 ────────────────────────────────────────────────────────

#[test]
fn every_reference_part_survives_roundtrip() {
    for name in [
        "1_text.pptx",
        "2_background.pptx",
        "3_picture.pptx",
        "4_merged.pptx",
        "5_cleaned.pptx",
        "sz_4x3.pptx",
        "sz_16x9.pptx",
    ] {
        let pkg = Package::open(probe(name)).unwrap_or_else(|e| panic!("{name} 開啟失敗：{e}"));
        let original: Vec<String> = pkg.part_names().map(str::to_owned).collect();

        let bytes = pkg.to_bytes().expect("序列化");
        let reopened = Package::from_bytes(&bytes).expect("重新讀取");

        let after: Vec<String> = reopened.part_names().map(str::to_owned).collect();
        assert_eq!(after, original, "{name}：part 清單在往返後改變");

        for part in &original {
            let a = pkg.part(part).unwrap();
            let b = reopened.part(part).unwrap();
            assert_eq!(a, b, "{name}：二進位 part {part} 往返後不同");

            if part.ends_with(".xml") || part.ends_with(".rels") {
                let doc_a = xml::parse(a).unwrap_or_else(|e| panic!("{name}/{part} 解析失敗：{e}"));
                let doc_b = xml::parse(b).unwrap();
                assert_eq!(
                    xml::normalize_for_compare(&doc_a.root),
                    xml::normalize_for_compare(&doc_b.root),
                    "{name}：XML part {part} 往返後結構不同"
                );
            }
        }
    }
}

#[test]
fn content_types_and_rels_are_intact() {
    let pkg = Package::open(probe("2_background.pptx")).unwrap();
    let rels = pkg.rels("ppt/slides/slide1.xml").unwrap();
    assert_eq!(rels.next_rid(), "rId3");
    assert_eq!(rels.target_of("rId2"), Some("../media/image1.png"));
    assert!(rels.items.iter().any(|r| r.rel_type == REL_TYPE_IMAGE));
    assert_eq!(
        Package::resolve_target("ppt/slides/slide1.xml", "../media/image1.png"),
        "ppt/media/image1.png"
    );
}

// ── 2. `p:sp` 寫入器 ─────────────────────────────────────────────────────────

#[test]
fn text_box_matches_spire_output() {
    let deck = Deck::open(probe("1_text.pptx")).unwrap();
    let reference = deck.slide(0).unwrap();
    let expected = find_sp(&reference, "Block_text");

    let ours = TextBox::new(
        "Block_text",
        "Hello MinerU",
        Rect::ltrb(60.0, 40.0, 660.0, 120.0),
    )
    .font("Calibri", 24.0)
    .align(n2p_ooxml::Align::Center)
    .color([0, 0, 0])
    .build(2);

    assert_same_structure(&ours, expected, "文字框");
}

#[test]
fn text_box_left_aligned_matches_paragraph_title_style() {
    // ppt_creater 對 paragraph_title 用 Left 對齊。
    let ours = TextBox::new(
        "Block_paragraph_title",
        "標題",
        Rect::ltrb(10.0, 10.0, 200.0, 40.0),
    )
    .font("Calibri", 16.0)
    .align(n2p_ooxml::Align::Left)
    .build(2);
    let algn = ours
        .path(&["p:txBody", "a:p", "a:pPr"])
        .and_then(|e| e.get_attr("algn"));
    assert_eq!(algn, Some("l"));
    // Spire 預設錨點是 ctr
    assert_eq!(
        ours.path(&["p:txBody", "a:bodyPr"])
            .and_then(|e| e.get_attr("anchor")),
        Some("ctr")
    );
}

#[test]
fn autofit_is_opt_in_because_spire_emits_none() {
    let default = TextBox::default().build(2);
    assert!(
        default
            .path(&["p:txBody", "a:bodyPr"])
            .unwrap()
            .child_el("a:normAutofit")
            .is_none(),
        "預設不應寫 autofit（與 Spire 等價）"
    );

    let with_autofit = TextBox::default()
        .autofit(n2p_ooxml::AutoFit::Normal)
        .build(2);
    assert!(
        with_autofit
            .path(&["p:txBody", "a:bodyPr"])
            .unwrap()
            .child_el("a:normAutofit")
            .is_some(),
        "AutoFit::Normal 應寫入 a:normAutofit"
    );
}

/// 段落結構摘要：`(a:p 數量, a:r 數量, a:br 數量, 串接後的文字)`。
fn paragraph_shape(text_box: &Element) -> (usize, usize, usize, String) {
    let tx_body = text_box.path(&["p:txBody"]).expect("p:txBody");
    let mut paragraphs = 0;
    let mut runs = 0;
    let mut breaks = 0;
    let mut text = String::new();
    for p in tx_body.elements("a:p") {
        paragraphs += 1;
        for child in &p.children {
            if let Node::Element(e) = child {
                match e.name.as_str() {
                    "a:r" => {
                        runs += 1;
                        if let Some(t) = e.child_el("a:t") {
                            if let Some(Node::Text(s)) = t.children.first() {
                                text.push_str(s);
                            }
                        }
                    }
                    "a:br" => breaks += 1,
                    _ => {}
                }
            }
        }
    }
    (paragraphs, runs, breaks, text)
}

#[test]
fn spire_keeps_newlines_inside_one_run_so_literal_mode_reproduces_it() {
    // 6_newline.pptx：Spire 對 TextFrame.Text = "line one\nline two" 的實際輸出
    let deck = Deck::open(probe("6_newline.pptx")).unwrap();
    let reference = deck.slide(0).unwrap();
    let expected = paragraph_shape(find_sp(&reference, "Block_text"));
    assert_eq!(
        expected,
        (1, 1, 0, "line one\nline two".to_string()),
        "基準檔的段落結構不如預期"
    );

    let literal = TextBox::new(
        "Block_text",
        "line one\nline two",
        Rect::ltrb(60.0, 40.0, 660.0, 120.0),
    )
    .newline(n2p_ooxml::NewlineMode::Literal)
    .build(2);
    assert_eq!(
        paragraph_shape(&literal),
        expected,
        "Literal 應與 Spire 的段落結構一致"
    );
}

#[test]
fn default_newline_mode_uses_a_br_and_is_visually_equivalent() {
    let text_box = TextBox::new(
        "Block_text",
        "line one\nline two",
        Rect::ltrb(60.0, 40.0, 660.0, 120.0),
    )
    .build(2);
    let (paragraphs, runs, breaks, text) = paragraph_shape(&text_box);
    assert_eq!(
        (paragraphs, runs, breaks),
        (1, 2, 1),
        "預設應為單段落、兩個 run、一個 a:br"
    );
    assert_eq!(text, "line oneline two");

    let paragraph_mode = TextBox::new(
        "Block_text",
        "line one\nline two",
        Rect::ltrb(60.0, 40.0, 660.0, 120.0),
    )
    .newline(n2p_ooxml::NewlineMode::Paragraph)
    .build(2);
    assert_eq!(
        paragraph_shape(&paragraph_mode).0,
        2,
        "Paragraph 模式應拆成兩段"
    );
}

// ── 3. `p:pic` 寫入器 ───────────────────────────────────────────────────────

#[test]
fn picture_matches_spire_output() {
    let deck = Deck::open(probe("3_picture.pptx")).unwrap();
    let reference = deck.slide(0).unwrap();
    let expected = find_pic(&reference);

    let ours = Picture::new("New picture", "rId2", Rect::ltrb(60.0, 40.0, 660.0, 120.0)).build(2);

    assert_same_structure(&ours, expected, "圖片");
}

// ── 4. `p:bg` 寫入器 ────────────────────────────────────────────────────────

#[test]
fn background_matches_spire_output() {
    let deck = Deck::open(probe("2_background.pptx")).unwrap();
    let reference = deck.slide(0).unwrap();
    let expected = n2p_ooxml::slide::background(&reference).expect("Spire 輸出應有 p:bg");

    let ours = Background::picture("rId2").build();

    assert_same_structure(&ours, expected, "背景");
}

// ── 5. 畫布尺寸 ─────────────────────────────────────────────────────────────

#[test]
fn slide_size_matches_spire_for_each_preset() {
    let cases = [
        ("sz_4x3.pptx", SlideSize::Screen4x3),
        ("sz_16x10.pptx", SlideSize::Screen16x10),
        ("sz_16x9.pptx", SlideSize::Screen16x9),
    ];

    for (name, size) in cases {
        // 參考值：直接讀 Spire 產出的檔
        let reference = Deck::open(probe(name)).unwrap();
        let expected = reference.slide_size().unwrap();
        assert_eq!(
            (expected.0, expected.1),
            size.emu(),
            "{name}：基準檔尺寸與預期常數不符"
        );

        // 從 4:3 的檔改成目標尺寸，結果應與 Spire 直接產生的相同
        let mut deck = Deck::open(probe("sz_4x3.pptx")).unwrap();
        deck.set_slide_size(size).unwrap();
        let actual = deck.slide_size().unwrap();
        assert_eq!(actual, expected, "{name}：sldSz 屬性不一致");
    }
}

#[test]
fn apply_pdf_page_size_matches_setup_presentation_strategy() {
    let mut deck = Deck::open(probe("sz_4x3.pptx")).unwrap();
    // 1000 x 562 → 1.779 > 1.65 → 16:9
    let chosen = deck.apply_pdf_page_size(1000.0, 562.0).unwrap();
    assert_eq!(chosen, SlideSize::Screen16x9);
    assert_eq!(deck.slide_size().unwrap().2.as_deref(), Some("screen16x9"));

    // 600 x 800 → 0.75 → 自訂，無 type 屬性
    let chosen = deck.apply_pdf_page_size(600.0, 800.0).unwrap();
    assert!(matches!(chosen, SlideSize::Custom { .. }));
    assert_eq!(deck.slide_size().unwrap().2, None);
}

// ── 6. 插入位置語意 ─────────────────────────────────────────────────────────

#[test]
fn insert_picture_at_bottom_lands_right_after_group_properties() {
    let mut deck = Deck::open(probe("src_a.pptx")).unwrap();
    let png = std::fs::read(probe("1_text.pptx")).unwrap(); // 內容不重要，只要是 bytes
    let rect = Rect::ltrb(10.0, 20.0, 110.0, 120.0);

    let (id, rid) = deck
        .insert_picture_at_bottom(0, png, "png", rect, "fg")
        .unwrap();
    assert!(rid.starts_with("rId"));

    let doc = deck.slide(0).unwrap();
    let tree = sp_tree(&doc);
    let kinds: Vec<&str> = tree
        .children
        .iter()
        .filter_map(|n| match n {
            Node::Element(e) => Some(e.name.as_str()),
            _ => None,
        })
        .collect();
    // src_*.pptx 由 probe.py 產生：1 個內容形狀（命名 "New shape"）+ 1 個 Spire 水印，
    // 兩者都是 p:sp，所以插入後應為 2 個 p:sp。
    assert_eq!(
        kinds,
        vec!["p:nvGrpSpPr", "p:grpSpPr", "p:pic", "p:sp", "p:sp"],
        "p:pic 應緊接在 p:grpSpPr 之後（ZOrderPosition = 0），既有 p:sp 被推到後面"
    );

    let pic = find_pic(&doc);
    assert_eq!(
        pic.path(&["p:nvPicPr", "p:cNvPr"])
            .and_then(|e| e.get_attr("id"))
            .and_then(|v| v.parse::<u32>().ok()),
        Some(id)
    );
}

#[test]
fn push_text_box_appends_last_with_next_id() {
    let mut deck = Deck::open(probe("src_a.pptx")).unwrap();
    let before = {
        let doc = deck.slide(0).unwrap();
        n2p_ooxml::slide::next_shape_id(&doc)
    };

    let id = deck
        .push_text_box(
            0,
            &TextBox::new("Block_extra", "追加", Rect::ltrb(1.0, 2.0, 3.0, 4.0)),
        )
        .unwrap();
    assert_eq!(id, before);

    let doc = deck.slide(0).unwrap();
    let names = n2p_ooxml::slide::shape_names(&doc).unwrap();
    assert_eq!(
        names.last().map(String::as_str),
        Some("Block_extra"),
        "新形狀應在最上層（最後）"
    );
}

// ── 7. 背景端到端：media + Content_Types + rels + p:bg ─────────────────────

#[test]
fn set_background_picture_wires_media_content_type_and_rels() {
    let mut deck = Deck::open(probe("src_a.pptx")).unwrap();
    let slide_part = slide_part_name(&deck);
    let png_bytes = std::fs::read(probe("1_text.pptx")).unwrap(); // 佔位用 bytes
    let before_parts: Vec<String> = deck.package().part_names().map(str::to_owned).collect();

    let rid = deck.set_background_picture(0, png_bytes, "png").unwrap();

    // 1. media part 產生
    let new_parts: Vec<String> = deck
        .package()
        .part_names()
        .filter(|p| !before_parts.iter().any(|b| b == p))
        .map(str::to_owned)
        .collect();
    assert!(
        new_parts.iter().any(|p| p.starts_with("ppt/media/")),
        "應新增 media part，實際：{new_parts:?}"
    );

    // 2. Content_Types 有 png 的 Default
    let ct = deck.package().xml("[Content_Types].xml").unwrap();
    assert!(
        ct.root
            .elements("Default")
            .any(|e| e.get_attr("Extension") == Some("png")),
        "Content_Types 應註冊 png"
    );

    // 3. rels 有 image 關聯，且 rId 就是回傳值
    let rels = deck.package().rels(&slide_part).unwrap();
    let rel = rels
        .items
        .iter()
        .find(|r| r.id == rid)
        .expect("rels 應含新 rId");
    assert_eq!(rel.rel_type, REL_TYPE_IMAGE);

    // 4. p:bg 是 p:cSld 的第一個子節點，且在 spTree 之前
    let doc = deck.slide(0).unwrap();
    let c_sld = doc.root.path(&["p:sld", "p:cSld"]).unwrap();
    assert_eq!(c_sld.element_len(), 2);
    assert_eq!(
        n2p_ooxml::slide::background(&doc).map(|e| e.name.as_str()),
        Some("p:bg")
    );
    assert!(
        matches!(&c_sld.children[0], Node::Element(e) if e.name == "p:bg"),
        "p:bg 必須排在 p:cSld 第一個子節點"
    );

    // 5. 產出的檔可被自己重新開啟（結構自洽）
    let bytes = deck.to_bytes().unwrap();
    let reopened = Deck::from_package(Package::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(reopened.slide_count().unwrap(), 1);
    assert!(n2p_ooxml::slide::background(&reopened.slide(0).unwrap()).is_some());
}

// ── 8. 浮水印：Spire 有、我們沒有 ────────────────────────────────────────────

#[test]
fn spire_output_has_evaluation_watermark_that_we_never_emit() {
    let deck = Deck::open(probe("4_merged.pptx")).unwrap();
    let mut watermark_pages = 0;
    for index in 0..deck.slide_count().unwrap() {
        let doc = deck.slide(index).unwrap();
        let text: String = doc
            .root
            .path(&["p:sld", "p:cSld", "p:spTree"])
            .map(|tree| {
                tree.elements("p:sp")
                    .filter_map(|sp| sp.path(&["p:txBody", "a:p", "a:r", "a:t"]))
                    .filter_map(|t| t.children.first())
                    .filter_map(|n| match n {
                        Node::Text(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        if text.contains("Evaluation Warning") {
            watermark_pages += 1;
        }
    }
    assert!(
        watermark_pages > 0,
        "基準檔應含 Spire 評估水印，否則這個測試沒有意義"
    );
}

#[test]
fn removing_spire_watermark_matches_clean_ppt_behaviour() {
    // 對照 assets/probe2.py 的實測結果：3 頁合併 → 刪掉 3 個 "New shape"，
    // 每頁的 Block_text 全部保留。
    let mut deck = Deck::open(probe("fix_merged.pptx")).unwrap();
    let pages = deck.slide_count().unwrap();
    assert_eq!(pages, 3);

    for index in 0..pages {
        let doc = deck.slide(index).unwrap();
        assert!(
            n2p_ooxml::slide::shape_names(&doc)
                .unwrap()
                .iter()
                .any(|n| n == "Block_text")
        );
    }

    let mut removed = 0;
    for index in 0..pages {
        removed += deck.remove_shapes_named(index, "New shape").unwrap();
    }
    assert_eq!(removed, 3, "應刪除 3 個水印形狀");

    for index in 0..pages {
        let doc = deck.slide(index).unwrap();
        let names = n2p_ooxml::slide::shape_names(&doc).unwrap();
        assert_eq!(
            names,
            vec!["Block_text".to_string()],
            "第 {index} 頁內容應完整保留"
        );
    }

    // 參照 fix_cleaned.pptx（來源形狀命名為 Block_text 的正確情境）
    let reference = Deck::open(probe("fix_cleaned.pptx")).unwrap();
    for index in 0..pages {
        let doc = reference.slide(index).unwrap();
        assert_eq!(
            n2p_ooxml::slide::shape_names(&doc).unwrap(),
            vec!["Block_text".to_string()],
            "與 Spire 的 clean_ppt 結果不一致（第 {index} 頁）"
        );
    }
}

#[test]
fn name_collision_hazard_is_reproducible_and_avoided_by_our_design() {
    // 5_cleaned.pptx 是 probe.py 的產物：那裡的來源形狀「也」命名為 "New shape"，
    // 與 Spire 評估水印同名，於是 clean_ppt 把內容一起刪光了 —— 三頁全空。
    //
    // 這正是 Python 版必須用 `shape.Name = f"Block_{label}"` 的原因，
    // 也是 Rust 版從設計上就免疫的地方（自己寫 XML，沒有水印可刪）。
    let collision = Deck::open(probe("5_cleaned.pptx")).unwrap();
    for index in 0..collision.slide_count().unwrap() {
        let doc = collision.slide(index).unwrap();
        assert!(
            n2p_ooxml::slide::shape_names(&doc).unwrap().is_empty(),
            "第 {index} 頁應被誤刪一空（碰撞情境）"
        );
        assert_eq!(
            n2p_ooxml::slide::shape_kinds(&doc).unwrap(),
            Vec::<&str>::new(),
            "第 {index} 頁 spTree 應已無任何形狀"
        );
    }

    // 我們自己產生的檔不含任何水印，因此 remove_shapes_named("New shape") 是空操作。
    let mut deck = Deck::open(probe("src_a.pptx")).unwrap();
    let before = n2p_ooxml::slide::shape_names(&deck.slide(0).unwrap())
        .unwrap()
        .len();
    assert_eq!(
        deck.remove_shapes_named(0, "Evaluation-Watermark").unwrap(),
        0
    );
    assert_eq!(
        n2p_ooxml::slide::shape_names(&deck.slide(0).unwrap())
            .unwrap()
            .len(),
        before
    );
}

// ── 9. 版面配置解析（合併同構捷徑的前提）────────────────────────────────────

#[test]
fn slide_layout_part_resolves_through_rels() {
    let deck = Deck::open(probe("src_a.pptx")).unwrap();
    let layout = deck.slide_layout_part(0).unwrap();
    assert!(
        layout.starts_with("ppt/slideLayouts/slideLayout"),
        "實際：{layout}"
    );
    assert!(deck.package().contains(&layout));

    // 三份同模板的來源應指向同一個 layout —— 這是合併時共用 master 的前提
    let a = Deck::open(probe("src_a.pptx")).unwrap();
    let b = Deck::open(probe("src_b.pptx")).unwrap();
    assert_eq!(
        a.slide_layout_part(0).unwrap(),
        b.slide_layout_part(0).unwrap()
    );

    // 合併檔（Spire 盲目匯入）三頁指向不同 layout —— 這是我們要避免的膨脹
    let merged = Deck::open(probe("4_merged.pptx")).unwrap();
    let layouts: Vec<String> = (0..3)
        .map(|i| merged.slide_layout_part(i).unwrap())
        .collect();
    assert_ne!(
        layouts[0], layouts[1],
        "Spire 合併後每頁各帶一組 layout（預期如此）"
    );
    assert_ne!(layouts[1], layouts[2]);
}

#[test]
fn same_template_layout_bytes_are_identical_so_dedup_is_safe() {
    let a = Deck::open(probe("src_a.pptx")).unwrap();
    let b = Deck::open(probe("src_c.pptx")).unwrap();
    let bytes_a = a.slide_layout_bytes(0).unwrap();
    let bytes_b = b.slide_layout_bytes(0).unwrap();
    assert_eq!(
        bytes_a, bytes_b,
        "同模板來源的 layout 內容應逐位元相同，才能安全去重"
    );
}
