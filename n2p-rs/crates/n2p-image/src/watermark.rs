//! NotebookLM 水印的定位與修補。
//!
//! 座標是**硬編碼**的，來自 `image_inpainter.py`：
//!
//! ```text
//! r1,r2,c1,c2 = 1536,1598,2627,2863
//! old_width, old_height = 2867,1600
//! ratio = image_width / old_width
//! assert abs(ratio - (image_height / old_height)) < 0.01
//! ```
//!
//! 這是與 NotebookLM 輸出尺寸的耦合點，移植時刻意原樣保留 ——
//! 改成「自動偵測」會讓去水印位置跑掉。

use n2p_core::{BBox, RgbImage};

use crate::diversity::{edge_average_color, edge_diversity};
use crate::inpaint;
use crate::mask::Mask;
use crate::method::InpaintMethod;

/// 基準影像寬度（`examples/floyd.pdf` 的 1376 pt × 150 dpi）。
pub const BASE_WIDTH: f64 = 2867.0;
/// 基準影像高度。
pub const BASE_HEIGHT: f64 = 1600.0;
/// 基準影像上的水印框：`left, top, right, bottom`。
pub const BASE_BOX: BBox = BBox {
    left: 2627,
    top: 1536,
    right: 2863,
    bottom: 1598,
};
/// 長寬比容許誤差（Python 的 `assert ... < 0.01`）。
pub const ASPECT_TOLERANCE: f64 = 0.01;
/// 低於此多樣性就直接填色，不做修補（Python 的 `diversity < 0.1`）。
pub const FLAT_BACKGROUND_THRESHOLD: f64 = 0.1;

/// 錯誤型別。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "影像長寬比與 NotebookLM 基準尺寸不符（{width}x{height}）：比例 {ratio:.4}，預期 {expected:.4}"
    )]
    AspectMismatch {
        width: u32,
        height: u32,
        ratio: f64,
        expected: f64,
    },
}

/// 依影像尺寸換算水印框。
///
/// 低邊取 floor、高邊取 ceil，與 Python 版一致。
pub fn watermark_box(width: u32, height: u32) -> Result<BBox, Error> {
    let ratio = width as f64 / BASE_WIDTH;
    let expected = height as f64 / BASE_HEIGHT;
    if (ratio - expected).abs() >= ASPECT_TOLERANCE {
        return Err(Error::AspectMismatch {
            width,
            height,
            ratio,
            expected,
        });
    }
    Ok(BBox::new(
        (BASE_BOX.left as f64 * ratio).floor() as i64,
        (BASE_BOX.top as f64 * ratio).floor() as i64,
        (BASE_BOX.right as f64 * ratio).ceil() as i64,
        (BASE_BOX.bottom as f64 * ratio).ceil() as i64,
    ))
}

/// 對指定矩形做修補，分派邏輯等同 `image_inpainter.inpaint_image`。
pub fn inpaint_region(image: &RgbImage, bbox: BBox, method: InpaintMethod) -> RgbImage {
    let (diversity, fill_color) = edge_diversity(image, bbox, 15);

    if diversity < FLAT_BACKGROUND_THRESHOLD || method == InpaintMethod::Background {
        let mask = Mask::from_bbox(image.width, image.height, bbox);
        let mut out = image.clone();
        for y in 0..image.height {
            for x in 0..image.width {
                if mask.get(x, y) {
                    out.set_pixel(x, y, fill_color);
                }
            }
        }
        return out;
    }

    let mask = Mask::from_bbox(image.width, image.height, bbox);
    match method {
        InpaintMethod::Background => unreachable!("上面已處理"),
        InpaintMethod::BackgroundSmooth => inpaint::manual(image, &mask, fill_color, 100),
        InpaintMethod::EdgeMeanSmooth => {
            let average = edge_average_color(image, bbox);
            inpaint::manual(image, &mask, average, 100)
        }
        InpaintMethod::Onion => inpaint::onion(image, &mask),
        InpaintMethod::Idw => inpaint::idw(image, &mask, 8, 2),
        InpaintMethod::Harmonic => inpaint::harmonic(image, &mask, 500),
    }
}

/// 去除 NotebookLM 水印。
pub fn inpaint_watermark(image: &RgbImage, method: InpaintMethod) -> Result<RgbImage, Error> {
    let bbox = watermark_box(image.width, image.height)?;
    Ok(inpaint_region(image, bbox, method))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_matches_python_hardcoded_values() {
        // 2867x1600 是 Python 版的基準，換算結果必須與硬編碼的數字完全相同
        let bbox = watermark_box(2867, 1600).unwrap();
        assert_eq!(
            (bbox.left, bbox.top, bbox.right, bbox.bottom),
            (2627, 1536, 2863, 1598)
        );
    }

    #[test]
    fn box_scales_with_dpi() {
        // 200 dpi：2867*200/150 ≈ 3823、1600*200/150 ≈ 2134
        let bbox = watermark_box(3823, 2134).unwrap();
        let ratio = 3823.0 / BASE_WIDTH;
        assert_eq!(bbox.left, (2627.0f64 * ratio).floor() as i64);
        assert_eq!(bbox.right, (2863.0f64 * ratio).ceil() as i64);
        assert!(bbox.right <= 3823, "右邊界不可超出影像");
    }

    #[test]
    fn aspect_mismatch_is_rejected() {
        let error = watermark_box(1000, 1000).unwrap_err();
        assert!(matches!(error, Error::AspectMismatch { .. }));
        // 1% 以內可接受
        assert!(watermark_box(2867, 1610).is_ok());
    }

    #[test]
    fn flat_background_takes_the_fast_path() {
        // 完全純色 → 多樣性 0 → 直接填色，不需修補
        let img = RgbImage::filled(2867, 1600, [240, 240, 240]);
        let out = inpaint_watermark(&img, InpaintMethod::Onion).unwrap();
        assert_eq!(out, img);
    }
}
