//! 影像比對與報告。

use n2p_core::RgbImage;

/// 一張圖與參考圖的差異。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageDiff {
    /// 單一通道的最大絕對差。
    pub max_channel: u8,
    /// 所有通道差的平均。
    pub mean_abs: f64,
    /// 任一通道差超過 `pixel_threshold` 的像素比例（0.0 ~ 1.0）。
    pub bad_ratio: f64,
    /// 總像素數。
    pub pixels: usize,
}

impl ImageDiff {
    /// 是否在容許範圍內。
    pub fn is_within(&self, limits: &Thresholds) -> bool {
        self.mean_abs <= limits.max_mean_abs && self.bad_ratio <= limits.max_bad_ratio
    }
}

/// 視覺回歸的容許範圍。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// 平均通道差上限。
    pub max_mean_abs: f64,
    /// 顯著差異像素比例上限。
    pub max_bad_ratio: f64,
    /// 單一像素視為「有差」的通道差門檻。
    pub pixel_threshold: u8,
}

impl Default for Thresholds {
    fn default() -> Self {
        // 兩個不同渲染引擎（LibreOffice 與原本的 Powerpoint/Spire）在抗鋸齒與
        // 字型上必然有差，因此門檻不能設在 0。預設值刻意寬鬆到「只看得出結構性差異」。
        Self {
            max_mean_abs: 6.0,
            max_bad_ratio: 0.10,
            pixel_threshold: 24,
        }
    }
}

/// 比較兩張圖。
pub fn diff(actual: &RgbImage, expected: &RgbImage, pixel_threshold: u8) -> Option<ImageDiff> {
    if actual.width != expected.width || actual.height != expected.height {
        return None;
    }
    if actual.data.len() != expected.data.len() || actual.data.is_empty() {
        return None;
    }

    let mut max_channel = 0u8;
    let mut total: u64 = 0;
    for (a, e) in actual.data.iter().zip(expected.data.iter()) {
        let delta = a.abs_diff(*e);
        max_channel = max_channel.max(delta);
        total += delta as u64;
    }

    let mut bad_pixels = 0usize;
    for (a, e) in actual
        .data
        .chunks_exact(3)
        .zip(expected.data.chunks_exact(3))
    {
        if a.iter()
            .zip(e.iter())
            .any(|(x, y)| x.abs_diff(*y) > pixel_threshold)
        {
            bad_pixels += 1;
        }
    }

    let pixels = actual.pixel_count();
    Some(ImageDiff {
        max_channel,
        mean_abs: total as f64 / actual.data.len() as f64,
        bad_ratio: bad_pixels as f64 / pixels as f64,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, color: [u8; 3]) -> RgbImage {
        RgbImage::filled(width, height, color)
    }

    #[test]
    fn identical_images_have_zero_diff() {
        let a = image(10, 10, [128, 128, 128]);
        let d = diff(&a, &a, 24).unwrap();
        assert_eq!(d.max_channel, 0);
        assert_eq!(d.mean_abs, 0.0);
        assert_eq!(d.bad_ratio, 0.0);
        assert!(d.is_within(&Thresholds::default()));
    }

    #[test]
    fn size_mismatch_is_reported_as_none() {
        let a = image(10, 10, [0, 0, 0]);
        let b = image(11, 10, [0, 0, 0]);
        assert!(diff(&a, &b, 24).is_none());
    }

    #[test]
    fn small_noise_stays_within_threshold() {
        let a = image(10, 10, [100, 100, 100]);
        let b = image(10, 10, [103, 98, 101]);
        let d = diff(&a, &b, 24).unwrap();
        assert_eq!(d.max_channel, 3);
        assert!(d.bad_ratio == 0.0);
        assert!(d.is_within(&Thresholds::default()));
    }

    #[test]
    fn structural_difference_fails() {
        let a = image(10, 10, [0, 0, 0]);
        let mut b = image(10, 10, [0, 0, 0]);
        // 把一半的畫面塗白，模擬「文字不見了」
        for y in 0..5 {
            for x in 0..10 {
                b.set_pixel(x, y, [255, 255, 255]);
            }
        }
        let d = diff(&a, &b, 24).unwrap();
        assert_eq!(d.max_channel, 255);
        assert!((d.bad_ratio - 0.5).abs() < 1e-9);
        assert!(!d.is_within(&Thresholds::default()));
    }

    #[test]
    fn pixel_threshold_controls_bad_ratio() {
        let a = image(4, 4, [100, 100, 100]);
        let b = image(4, 4, [130, 100, 100]);
        assert_eq!(
            diff(&a, &b, 24).unwrap().bad_ratio,
            1.0,
            "差 30 > 24 應算有差"
        );
        assert_eq!(
            diff(&a, &b, 40).unwrap().bad_ratio,
            0.0,
            "差 30 < 40 應算沒差"
        );
    }
}
