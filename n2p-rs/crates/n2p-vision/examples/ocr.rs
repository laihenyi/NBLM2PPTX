//! 對一張影像做 Vision OCR。
//!
//! ```bash
//! cargo run -p n2p-vision --example ocr -- <image.png>
//! ```

use n2p_core::RgbImage;
use n2p_layout::LayoutProvider;
use n2p_vision::{VisionConfig, VisionDocument, recognize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/n2p-ocr.png".into());
    let image = RgbImage::open(&path)?;
    println!("{path}：{} x {}", image.width, image.height);

    let config = VisionConfig::default();
    let observations = recognize(&image, &config)?;
    println!("辨識 {} 筆：", observations.len());
    for (index, observation) in observations.iter().enumerate() {
        println!(
            "  [{index}] conf={:.2} bbox=({:.3},{:.3},{:.3},{:.3}) {:?}",
            observation.confidence,
            observation.bbox_norm.0,
            observation.bbox_norm.1,
            observation.bbox_norm.2,
            observation.bbox_norm.3,
            observation.text
        );
    }

    let doc = VisionDocument::from_images(&[image], &[(800.0, 600.0)], &config)?;
    let page = doc.page(0)?;
    println!("\n版面（{} 個區塊）：", page.blocks.len());
    for block in &page.blocks {
        println!("  {:?} {:?}", block.bbox, block.text);
    }
    Ok(())
}
