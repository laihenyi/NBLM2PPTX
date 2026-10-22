//! 診斷用：對單張 PNG 的指定區域跑「分析 → 遮罩 → LaMa 修補」，
//! 並寫出遮罩與修補後的圖，方便肉眼比對。
//!
//! ```sh
//! cargo run -p n2p-image --example lama_debug -- \
//!     /tmp/page.png 355 1058 505 80 /tmp/out
//! ```

use n2p_core::{BBox, RgbImage};
use n2p_image::{Lama, textmask};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("用法：image x y w h out_prefix [glow_px] [line_height_px]")?;
    let x: i64 = args.next().ok_or("缺少 x")?.parse()?;
    let y: i64 = args.next().ok_or("缺少 y")?.parse()?;
    let w: i64 = args.next().ok_or("缺少 w")?.parse()?;
    let h: i64 = args.next().ok_or("缺少 h")?.parse()?;
    let out = args.next().ok_or("缺少輸出前綴")?;
    let glow: u32 = args.next().unwrap_or_else(|| "0".into()).parse()?;
    // 單行字高（像素）；預設 h/2，單行區塊請明確指定
    let line_height: f64 = match args.next() {
        Some(value) => value.parse()?,
        None => h as f64 / 2.0,
    };

    let bytes = std::fs::read(&path)?;
    let mut image = RgbImage::from_png_bytes(&bytes)?;
    println!("影像 {}x{}", image.width, image.height);

    let bbox = BBox::new(x, y, x + w, y + h);
    let analysis = textmask::analyze_with_line_height(&image, bbox, line_height);
    println!(
        "密度 {:.3}  顏色 {:?}  粗體 {}",
        analysis.density, analysis.color, analysis.is_bold
    );

    // 把遮罩畫出來
    let mut debug = image.clone();
    for py in 0..debug.height {
        for px in 0..debug.width {
            if analysis.mask.get(px, py) {
                debug.set_pixel(px, py, [255, 0, 255]);
            }
        }
    }
    std::fs::write(format!("{out}_mask.png"), debug.to_png()?)?;

    let mut mask = analysis.mask.clone();
    mask.dilate(glow);

    let mut engine = Lama::load_default()?;
    let started = std::time::Instant::now();
    engine.inpaint_mask(&mut image, &mask)?;
    println!("修補耗時 {:?}", started.elapsed());
    std::fs::write(format!("{out}_after.png"), image.to_png()?)?;
    Ok(())
}
