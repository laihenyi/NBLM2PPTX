//! 影像幾何轉換：16:9 寬屏化。
//!
//! 移植自 `pdf2png.py` 的 `make_wide_screen` 分支與 `ppt_creater.py` 的
//! `make_data_wide_screen`（前者動圖片，後者動 MinerU JSON 的座標）。

use n2p_core::RgbImage;

/// 目標寬高比 16:9。
pub const WIDESCREEN_RATIO: f64 = 16.0 / 9.0;

/// 對應 Python `round()`：四捨六入、**.5 取偶**。
///
/// Rust 的 `f64::round()` 是「.5 遠離零」，與 Python 不同；這裡照 Python 行為實作，
/// 以免寬屏化時差一個像素。
pub fn round_half_even(value: f64) -> i64 {
    let floor = value.floor();
    let fraction = value - floor;
    let result = if (fraction - 0.5).abs() < f64::EPSILON {
        // 取偶
        if (floor as i64) % 2 == 0 {
            floor
        } else {
            floor + 1.0
        }
    } else {
        value.round()
    };
    result as i64
}

/// 把圖片調整成 16:9：不足則置中補白，過寬則置中裁切。
///
/// 已是 16:9 時原樣回傳（等同 Python 版的「無需調整」分支）。
pub fn to_widescreen(image: &RgbImage) -> RgbImage {
    let width = image.width as i64;
    let height = image.height as i64;
    let target_width = round_half_even(height as f64 * WIDESCREEN_RATIO);

    if target_width == width {
        return image.clone();
    }

    if target_width > width {
        // 擴寬並置中補白
        let mut out = RgbImage::filled(target_width as u32, image.height, [255, 255, 255]);
        let offset = ((target_width - width) / 2) as u32;
        for y in 0..image.height {
            let src = image.row(y);
            let dst = out.row_mut(y);
            let start = (offset as usize) * 3;
            dst[start..start + src.len()].copy_from_slice(src);
        }
        return out;
    }

    // 裁切左右
    let left = (width - target_width) / 2;
    let mut out = RgbImage::new(target_width as u32, image.height);
    for y in 0..image.height {
        let src = image.row(y);
        let dst = out.row_mut(y);
        let start = (left as usize) * 3;
        dst.copy_from_slice(&src[start..start + (target_width as usize) * 3]);
    }
    out
}

/// 是否已是 16:9。
pub fn is_widescreen(image: &RgbImage) -> bool {
    round_half_even(image.height as f64 * WIDESCREEN_RATIO) == image.width as i64
}

/// 把一個邊界框從原始座標換算到寬屏化後的座標。
///
/// 對應 `make_data_wide_screen`：擴寬時所有 x 座標右移，裁切時左移。
pub fn shift_bbox_for_widescreen(
    bbox: n2p_core::BBox,
    source_width: u32,
    height: u32,
) -> n2p_core::BBox {
    let target_width = round_half_even(height as f64 * WIDESCREEN_RATIO);
    let width = source_width as i64;
    let offset = if target_width > width {
        (target_width - width) / 2
    } else {
        -((width - target_width) / 2)
    };
    n2p_core::BBox::new(
        bbox.left + offset,
        bbox.top,
        bbox.right + offset,
        bbox.bottom,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_rounding_is_half_even() {
        assert_eq!(round_half_even(0.5), 0);
        assert_eq!(round_half_even(1.5), 2);
        assert_eq!(round_half_even(2.5), 2);
        assert_eq!(round_half_even(3.5), 4);
        assert_eq!(round_half_even(2844.44), 2844);
    }

    #[test]
    fn widen_pads_centered_with_white() {
        let img = RgbImage::filled(100, 100, [10, 20, 30]);
        let out = to_widescreen(&img);
        // 100 * 16/9 = 177.78 → 178
        assert_eq!(out.width, 178);
        assert_eq!(out.height, 100);
        assert_eq!(out.pixel(0, 0), [255, 255, 255]);
        // 置中：offset = (178-100)/2 = 39
        assert_eq!(out.pixel(39, 0), [10, 20, 30]);
        assert_eq!(out.pixel(138, 0), [10, 20, 30]);
        assert_eq!(out.pixel(139, 0), [255, 255, 255]);
    }

    #[test]
    fn crop_keeps_center() {
        let mut img = RgbImage::filled(200, 100, [0, 0, 0]);
        img.set_pixel(100, 50, [9, 9, 9]);
        let out = to_widescreen(&img);
        assert_eq!((out.width, out.height), (178, 100));
        // 原 x=100 變成 100-11=89
        assert_eq!(out.pixel(89, 50), [9, 9, 9]);
    }

    #[test]
    fn already_widescreen_is_untouched() {
        let img = RgbImage::filled(160, 90, [1, 2, 3]);
        assert!(is_widescreen(&img));
        assert_eq!(to_widescreen(&img), img);
    }

    #[test]
    fn bbox_shifts_match_padding() {
        let bbox = n2p_core::BBox::new(10, 5, 20, 15);
        let shifted = shift_bbox_for_widescreen(bbox, 100, 100);
        assert_eq!(shifted, n2p_core::BBox::new(49, 5, 59, 15));
    }
}
