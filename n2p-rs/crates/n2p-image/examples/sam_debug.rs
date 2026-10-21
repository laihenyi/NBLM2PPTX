//! 診斷用：對單張 PNG 跑 MobileSAM 物件偵測，輸出每個物件的外接框與彩色疊圖。
//!
//! ```sh
//! cargo run --release -p n2p-image --example sam_debug -- /tmp/page.png /tmp/out [grid]
//! ```

use n2p_core::RgbImage;
use n2p_image::{Lama, sam};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("用法：image out_prefix [grid]")?;
    let out = args.next().ok_or("缺少輸出前綴")?;
    let grid: usize = args.next().unwrap_or_else(|| "16".into()).parse()?;

    // 同一行程先載入 LaMa，確認兩個模型能共用 ONNX Runtime 環境。
    let lama = Lama::load_default();
    println!(
        "LaMa：{}",
        if lama.is_ok() {
            "已載入"
        } else {
            "未載入"
        }
    );

    let image = RgbImage::from_png_bytes(&std::fs::read(&path)?)?;
    let mut engine = sam::Sam::load_default()?;
    if std::env::var_os("N2P_SAM_LOAD_ONLY").is_some() {
        println!("只載入模型");
        return Ok(());
    }
    let defaults = sam::SamOptions::default();
    // 除錯用門檻覆寫
    let env = |name: &str, fallback: f32| -> f32 {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(fallback)
    };
    let options = sam::SamOptions {
        grid,
        min_iou: env("N2P_SAM_IOU", defaults.min_iou),
        min_stability: env("N2P_SAM_STAB", defaults.min_stability),
        max_area_fraction: env("N2P_SAM_MAX_AREA", defaults.max_area_fraction as f32) as f64,
        ..defaults
    };
    let started = std::time::Instant::now();
    let objects = engine.segment(&image, &options)?;
    println!("{} 個物件，耗時 {:?}", objects.len(), started.elapsed());

    const PALETTE: [[u8; 3]; 8] = [
        [255, 0, 255],
        [0, 255, 0],
        [255, 128, 0],
        [0, 160, 255],
        [255, 255, 0],
        [255, 0, 0],
        [0, 255, 255],
        [160, 0, 255],
    ];
    let mut overlay = image.clone();
    for (index, object) in objects.iter().enumerate() {
        let b = object.bbox;
        println!(
            "  [{index}] score={:.3} bbox=({}, {})–({}, {}) 面積={:.2}%",
            object.score,
            b.left,
            b.top,
            b.right,
            b.bottom,
            object.mask.count() as f64 * 100.0 / (image.width * image.height) as f64
        );
        if std::env::var_os("N2P_SAM_DUMP").is_some() {
            // 單一物件剪影：物件外塗成洋紅色。
            let mut cut = image.clone();
            for y in 0..cut.height {
                for x in 0..cut.width {
                    if !object.mask.get(x, y) {
                        cut.set_pixel(x, y, [255, 0, 255]);
                    }
                }
            }
            std::fs::write(format!("{out}_obj{index}.png"), cut.to_png()?)?;
        }
        let color = PALETTE[index % PALETTE.len()];
        for y in b.top as u32..b.bottom as u32 {
            for x in b.left as u32..b.right as u32 {
                if object.mask.get(x, y) {
                    let p = overlay.pixel(x, y);
                    overlay.set_pixel(
                        x,
                        y,
                        [
                            ((p[0] as u16 + color[0] as u16) / 2) as u8,
                            ((p[1] as u16 + color[1] as u16) / 2) as u8,
                            ((p[2] as u16 + color[2] as u16) / 2) as u8,
                        ],
                    );
                }
            }
        }
    }
    std::fs::write(format!("{out}_objects.png"), overlay.to_png()?)?;
    Ok(())
}
