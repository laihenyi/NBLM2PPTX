//! 對照 PyMuPDF 的參考測試。
//!
//! 關鍵驗證點是**幾何**：`image_inpainter.py` 的水印座標以 `2867 x 1600` 為基準
//! （`examples/floyd.pdf` 的 1376 x 768 pt @ 150 dpi）。若點陣化尺寸差一個像素，
//! 去水印位置就會位移，因此這裡把它鎖成測試。

use std::path::PathBuf;

use n2p_pdf::PdfDoc;

fn pdf_path() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../examples/floyd.pdf");
    assert!(path.exists(), "缺少 {path:?}");
    path
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("n2p-pdf-tests").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建立暫存目錄");
    dir
}

fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("PNG header");
    let mut buf = vec![0u8; reader.output_buffer_size().expect("buffer size")];
    let info = reader.next_frame(&mut buf).expect("PNG frame");
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

#[test]
fn page_count_and_size_match_pymupdf() {
    let doc = PdfDoc::open(pdf_path()).unwrap();
    assert_eq!(doc.page_count(), 2, "MuPDF 對這份 PDF 也回報 2 頁");
    for index in 0..doc.page_count() {
        let (w, h) = doc.page_size_pt(index).unwrap();
        assert_eq!(
            (w, h),
            (1376.0, 768.0),
            "第 {index} 頁尺寸應與 MuPDF 的 page.rect 相同"
        );
    }
    assert_eq!(doc.first_page_size_pt().unwrap(), (1376.0, 768.0));
}

#[test]
fn pixel_size_rounds_up_like_mupdf() {
    let doc = PdfDoc::open(pdf_path()).unwrap();
    // 1376 * 150/72 = 2866.67 → MuPDF 取 2867；截斷會得到 2866。
    // image_inpainter.py 硬編碼的 old_width/old_height 正是 2867/1600。
    assert_eq!(doc.pixel_size(0, 150.0).unwrap(), (2867, 1600));
    // 整數倍的情況不受影響
    assert_eq!(doc.pixel_size(0, 72.0).unwrap(), (1376, 768));
    assert_eq!(doc.pixel_size(0, 144.0).unwrap(), (2752, 1536));
    // 200 dpi：1376*200/72=3822.2→3823；768*200/72=2133.3→2134
    assert_eq!(doc.pixel_size(0, 200.0).unwrap(), (3823, 2134));
}

#[test]
fn rendered_bitmap_has_exact_target_dimensions() {
    let doc = PdfDoc::open(pdf_path()).unwrap();
    let image = doc.render_rgb(0, 150.0).unwrap();
    assert_eq!((image.width, image.height), (2867, 1600));
    assert_eq!(image.data.len(), 2867 * 1600 * 3);
    assert_eq!(image.row(0).len(), 2867 * 3);
    // 內容不是空白（否則後續去水印/OCR 全部沒意義）
    let distinct: std::collections::HashSet<u8> = image.data.iter().copied().collect();
    assert!(
        distinct.len() > 8,
        "渲染結果應有實際內容，實際只有 {} 種色階",
        distinct.len()
    );
}

#[test]
fn png_roundtrip_keeps_dimensions() {
    let doc = PdfDoc::open(pdf_path()).unwrap();
    let png = doc.render_png(1, 150.0).unwrap();
    let (w, h, rgb) = decode_png(&png);
    assert_eq!((w, h), (2867, 1600));
    assert_eq!(rgb.len(), 2867 * 1600 * 3);
}

#[test]
fn render_to_dir_uses_python_compatible_names() {
    let doc = PdfDoc::open(pdf_path()).unwrap();
    let dir = temp_dir("names");
    let names = doc.render_to_dir(&dir, 150.0, None, true).unwrap();
    assert_eq!(
        names,
        vec!["page_0001.png".to_string(), "page_0002.png".to_string()]
    );
    for name in &names {
        assert!(dir.join(name).exists(), "{name} 應存在");
    }
}

#[test]
fn pages_filter_is_one_based_like_the_python_api() {
    let doc = PdfDoc::open(pdf_path()).unwrap();
    let dir = temp_dir("filter");
    let names = doc.render_to_dir(&dir, 72.0, Some(&[2]), true).unwrap();
    assert_eq!(names, vec!["page_0002.png".to_string()]);
    assert!(!dir.join("page_0001.png").exists());
}

#[test]
fn force_regenerate_false_skips_existing_files() {
    let doc = PdfDoc::open(pdf_path()).unwrap();
    let dir = temp_dir("skip");
    doc.render_to_dir(&dir, 72.0, None, true).unwrap();

    let before = std::fs::metadata(dir.join("page_0001.png")).unwrap().len();
    let stamp_before = std::fs::metadata(dir.join("page_0001.png"))
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));

    let names = doc.render_to_dir(&dir, 72.0, None, false).unwrap();
    assert_eq!(names.len(), 2, "跳過時仍應回報檔名");
    let stamp_after = std::fs::metadata(dir.join("page_0001.png"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(stamp_before, stamp_after, "既有檔案不應被重寫");
    assert_eq!(
        before,
        std::fs::metadata(dir.join("page_0001.png")).unwrap().len()
    );
}

#[test]
fn invalid_dpi_is_rejected() {
    let doc = PdfDoc::open(pdf_path()).unwrap();
    assert!(doc.pixel_size(0, 0.0).is_err());
    assert!(doc.pixel_size(0, f64::NAN).is_err());
    assert!(doc.render_rgb(0, -1.0).is_err());
}

#[test]
fn out_of_range_page_is_rejected() {
    let doc = PdfDoc::open(pdf_path()).unwrap();
    let error = doc.page_size_pt(9).unwrap_err().to_string();
    assert!(error.contains("超出範圍"), "實際：{error}");
}
