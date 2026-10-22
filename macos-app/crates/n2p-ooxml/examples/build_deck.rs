//! 端到端煙霧測試：用三個寫入器把一頁「假投影片」組出來。
//!
//! ```bash
//! cargo run --example build_deck -- <template.pptx> <image.png> <out.pptx>
//! ```
//!
//! 預設會拿 `fixtures/probe-pptx/src_a.pptx` 當範本，
//! 輸出 `/tmp/n2p-example.pptx`。產出可用 `python-pptx` 獨立驗證：
//!
//! ```bash
//! python3 -c "from pptx import Presentation; p=Presentation('/tmp/n2p-example.pptx'); \
//!   print(p.slide_width, p.slide_height, len(p.slides[0].shapes))"
//! ```

use std::path::PathBuf;

use n2p_ooxml::{Align, AutoFit, Deck, Rect, SlideSize, TextBox};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/probe-pptx");

    let template = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| base.join("src_a.pptx"));
    let image = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| base.join("../test.png"));
    let out = args
        .get(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/n2p-example.pptx"));

    let png = std::fs::read(&image)
        .map_err(|e| format!("讀不到影像 {image:?}：{e}（可用 PIL 產生一張測試圖）"))?;

    let mut deck = Deck::open(&template)?;
    println!("範本：{template:?}（{} 頁）", deck.slide_count()?);

    // 1. 畫布：16:9
    deck.set_slide_size(SlideSize::Screen16x9)?;
    println!("畫布：{:?}", deck.slide_size()?);

    // 2. 標題文字框（左對齊，對應 paragraph_title）
    let title = TextBox::new(
        "Block_paragraph_title",
        "以 Rust 重寫的 PPTX 引擎",
        Rect::ltrb(60.0, 30.0, 660.0, 80.0),
    )
    .font("Microsoft JhengHei", 20.0)
    .align(Align::Left)
    .autofit(AutoFit::Normal);
    deck.push_text_box(0, &title)?;

    // 3. 內文文字框（置中，多行）
    let body = TextBox::new(
        "Block_text",
        "只有 zip + quick-xml，沒有 Spire\n沒有 Python runtime\n沒有評估水印",
        Rect::ltrb(60.0, 400.0, 660.0, 520.0),
    )
    .font("Calibri", 16.0)
    .align(Align::Center);
    deck.push_text_box(0, &body)?;

    // 4. 前景圖置底（對應 extract_foreground_element 的 ZOrderPosition = 0）
    let (pic_id, pic_rid) = deck.insert_picture_at_bottom(
        0,
        png.clone(),
        "png",
        Rect::ltrb(120.0, 100.0, 600.0, 380.0),
        "fg",
    )?;
    println!("前景圖：shape id={pic_id} rId={pic_rid}");

    // 5. 背景圖
    let bg_rid = deck.set_background_picture(0, png, "png")?;
    println!("背景圖：rId={bg_rid}");

    deck.save(&out)?;
    let size = std::fs::metadata(&out)?.len();
    println!("輸出：{out:?}（{size} bytes）");

    // 自我檢查：重新開啟並列出結構
    let reopened = Deck::open(&out)?;
    let doc = reopened.slide(0)?;
    println!("形狀種類：{:?}", n2p_ooxml::slide::shape_kinds(&doc)?);
    println!("形狀名稱：{:?}", n2p_ooxml::slide::shape_names(&doc)?);
    println!("有背景：{}", n2p_ooxml::slide::background(&doc).is_some());

    Ok(())
}
