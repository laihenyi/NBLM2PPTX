//! 把 PDF 每頁渲染成 PNG。
//!
//! ```bash
//! cargo run -p n2p-pdf --example render -- <input.pdf> <out_dir> [dpi]
//! ```

use n2p_pdf::PdfDoc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let input = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("../../../examples/floyd.pdf");
    let out = args
        .get(2)
        .map(String::as_str)
        .unwrap_or("/tmp/n2p-pdf-out");
    let dpi: f64 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(150.0);

    let doc = PdfDoc::open(input)?;
    println!(
        "{input}：{} 頁，{} bytes",
        doc.page_count(),
        doc.bytes_len()
    );
    for index in 0..doc.page_count() {
        let (w, h) = doc.page_size_pt(index)?;
        let image = doc.render_rgb(index, dpi)?;
        println!(
            "  page {}：{w:.2} x {h:.2} pt（{:.1} x {:.1} in）→ {} x {} px @ {dpi} dpi",
            index + 1,
            w / 72.0,
            h / 72.0,
            image.width,
            image.height
        );
    }
    let names = doc.render_to_dir(out, dpi, None, true)?;
    println!("輸出 {} 個 PNG 到 {out}", names.len());
    Ok(())
}
