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

/// 就地修補一個矩形區域，只動 `bbox` 之內的像素。
///
/// 與 [`inpaint_region`] 的差別是**不做整張影像的複製**：內部只裁出
/// `bbox` 加 padding 的小區塊來處理再貼回。對「一頁有數十個文字區塊要擦除」
/// 的情境，這決定了成本是「區塊面積」還是「整頁面積」。
pub fn inpaint_bbox_in_place(image: &mut RgbImage, bbox: BBox, method: InpaintMethod) {
    let padded = BBox::new(bbox.left - 2, bbox.top - 2, bbox.right + 2, bbox.bottom + 2)
        .clamp(image.width as i64, image.height as i64);
    if padded.is_empty() {
        return;
    }
    let crop = image.crop(padded);
    let local = BBox::new(
        bbox.left - padded.left,
        bbox.top - padded.top,
        bbox.right - padded.left,
        bbox.bottom - padded.top,
    );
    let cleaned = inpaint_region(&crop, local, method);
    for y in 0..crop.height {
        for x in 0..crop.width {
            image.set_pixel(
                padded.left as u32 + x,
                padded.top as u32 + y,
                cleaned.pixel(x, y),
            );
        }
    }
}

/// 依遮罩就地修補（只動遮罩為真的像素）。
///
/// 與 [`inpaint_bbox_in_place`] 的差別：這裡的遮罩是**逐像素**的，不是整個矩形。
/// 用途是「只移除文字筆畫」—— 同一區域裡的彩色圖示、外框光暈因為不符遮罩條件，
/// 一個像素都不會被動到。
pub fn inpaint_mask_in_place(image: &mut RgbImage, mask: &Mask, method: InpaintMethod) {
    let Some(bbox) = mask.bbox() else {
        return;
    };
    // 裁剪範圍要比遮罩大一圈，否則「非遮罩像素」仍全是文字周圍的光暈／反鋸齒，
    // 取來的主色會是白色（CH7 第 5 頁的深色背景上留下一條白帶）。
    let pad = (bbox.height() / 3).max(6);
    let padded = BBox::new(
        bbox.left - pad,
        bbox.top - pad,
        bbox.right + pad,
        bbox.bottom + pad,
    )
    .clamp(image.width as i64, image.height as i64);
    let crop = image.crop(padded);
    let mut local = Mask::new(crop.width, crop.height);
    for y in 0..crop.height {
        for x in 0..crop.width {
            if mask.get(padded.left as u32 + x, padded.top as u32 + y) {
                local.set(x, y, true);
            }
        }
    }
    // 填充色從「遮罩以外的像素」取主色。
    //
    // 不能由呼叫端從 bbox 外圍取樣：文字帶白色光暈時，那一圈仍是光暈 → 填成白色，
    // 於是在深色背景上留下一條白帶（CH7 第 5 頁實測）。
    let background = dominant_unmasked_color(&crop, &local);
    let cleaned = match method {
        InpaintMethod::Onion => inpaint::onion(&crop, &local),
        InpaintMethod::Harmonic => inpaint::harmonic(&crop, &local, 500),
        InpaintMethod::Idw => inpaint::idw(&crop, &local, 8, 2),
        InpaintMethod::EdgeMeanSmooth => {
            let average = edge_average_color(&crop, padded_bbox_of(&local));
            inpaint::manual(&crop, &local, average, 100)
        }
        _ => inpaint::manual(&crop, &local, background, 100),
    };
    for y in 0..crop.height {
        for x in 0..crop.width {
            if local.get(x, y) {
                image.set_pixel(
                    padded.left as u32 + x,
                    padded.top as u32 + y,
                    cleaned.pixel(x, y),
                );
            }
        }
    }
}

fn padded_bbox_of(mask: &Mask) -> BBox {
    mask.bbox().unwrap_or(BBox::new(0, 0, 0, 0))
}

/// 遮罩以外像素的主色（量化到 16 階後取眾數再平均）。
fn dominant_unmasked_color(image: &RgbImage, mask: &Mask) -> [u8; 3] {
    let mut buckets: std::collections::HashMap<[u8; 3], (usize, [u64; 3])> =
        std::collections::HashMap::new();
    for y in 0..image.height {
        for x in 0..image.width {
            if mask.get(x, y) {
                continue;
            }
            let pixel = image.pixel(x, y);
            let key = [pixel[0] & 0xF0, pixel[1] & 0xF0, pixel[2] & 0xF0];
            let entry = buckets.entry(key).or_insert((0, [0; 3]));
            entry.0 += 1;
            entry.1[0] += pixel[0] as u64;
            entry.1[1] += pixel[1] as u64;
            entry.1[2] += pixel[2] as u64;
        }
    }
    match buckets.into_values().max_by_key(|(count, _)| *count) {
        Some((count, sums)) => [
            (sums[0] / count as u64) as u8,
            (sums[1] / count as u64) as u8,
            (sums[2] / count as u64) as u8,
        ],
        None => [255, 255, 255],
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
