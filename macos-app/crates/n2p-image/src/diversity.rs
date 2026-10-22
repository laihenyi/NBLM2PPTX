//! 邊緣色彩統計。
//!
//! 移植自 `notebooklm2ppt/utils/edge_diversity.py`。原版以 numpy 實作，
//! 並刻意取代了早期版本的 DBSCAN：把顏色「量化到桶」再取眾數，
//! 效果等價而成本低得多。
//!
//! 數值行為必須與 numpy 版逐位元相同，因為下游用它決定「這個區塊是純色
//! 背景還是複雜圖片」，一旦判定翻轉，去水印的結果就會不同。

use std::collections::BTreeMap;

use n2p_core::{BBox, RgbImage};

/// 一個像素的 RGB。
pub type Rgb = [u8; 3];

/// 收集矩形四邊的像素（上、下、左、右，順序與 numpy 版一致）。
///
/// numpy 版的切片是：
/// ```text
/// top    = image[top:top+1,       left:right]
/// bottom = image[bottom-1:bottom, left:right]
/// left   = image[top:bottom,      left:left+1]
/// right  = image[top:bottom,      right-1:right]
/// ```
/// 空切片會被過濾，這裡以「範圍為空就略過」等價處理。
pub fn edge_pixels(image: &RgbImage, bbox: BBox) -> Vec<Rgb> {
    let bbox = bbox.clamp(image.width as i64, image.height as i64);
    let mut out = Vec::new();
    if bbox.is_empty() {
        return out;
    }
    let (l, t, r, b) = (bbox.left, bbox.top, bbox.right, bbox.bottom);

    // top: y = t, x in [l, r)
    for x in l..r {
        out.push(image.pixel(x as u32, t as u32));
    }
    // bottom: y = b-1, x in [l, r)
    for x in l..r {
        out.push(image.pixel(x as u32, (b - 1) as u32));
    }
    // left: x = l, y in [t, b)
    for y in t..b {
        out.push(image.pixel(l as u32, y as u32));
    }
    // right: x = r-1, y in [t, b)
    for y in t..b {
        out.push(image.pixel((r - 1) as u32, y as u32));
    }
    out
}

/// 計算邊緣顏色多樣性與主色。
///
/// * `diversity`：`1 - (最大顏色桶佔比)`。值越小代表背景越單純。
/// * `fill_color`：主色桶內所有原始像素的平均色（截斷取整，與 numpy 的
///   `astype(np.uint8)` 一致）。
///
/// 回傳 `(1.0, [255,255,255])` 代表「邊緣取樣為空」，與 numpy 版相同。
pub fn edge_diversity(image: &RgbImage, bbox: BBox, tolerance: u8) -> (f64, Rgb) {
    let points = edge_pixels(image, bbox);
    if points.is_empty() {
        return (1.0, [255, 255, 255]);
    }

    // 顏色量化：整除 tolerance。tolerance 為 0 時退化為「完全相同的顏色才同桶」。
    let divisor = tolerance.max(1) as u16;
    let quantize = |rgb: Rgb| -> (u8, u8, u8) {
        (
            (rgb[0] as u16 / divisor) as u8,
            (rgb[1] as u16 / divisor) as u8,
            (rgb[2] as u16 / divisor) as u8,
        )
    };

    // `np.unique(..., return_counts=True)` 會依量化色排序；`argmax` 取第一個最大值，
    // 因此平手時由「字典序最小的桶」勝出。BTreeMap 的迭代順序正好對應。
    let mut buckets: BTreeMap<(u8, u8, u8), (usize, [u64; 3])> = BTreeMap::new();
    for rgb in &points {
        let entry = buckets.entry(quantize(*rgb)).or_insert((0, [0, 0, 0]));
        entry.0 += 1;
        entry.1[0] += rgb[0] as u64;
        entry.1[1] += rgb[1] as u64;
        entry.1[2] += rgb[2] as u64;
    }

    let mut dominant = (0u8, 0u8, 0u8);
    let mut best_count = 0usize;
    for (key, (count, _)) in &buckets {
        if *count > best_count {
            best_count = *count;
            dominant = *key;
        }
    }

    let total = points.len();
    let (_, sums) = buckets[&dominant];
    let main_ratio = best_count as f64 / total as f64;
    let fill_color = [
        truncate_div(sums[0], best_count),
        truncate_div(sums[1], best_count),
        truncate_div(sums[2], best_count),
    ];

    (1.0 - main_ratio, fill_color)
}

/// 邊緣平均色（`compute_edge_average_color`）。
pub fn edge_average_color(image: &RgbImage, bbox: BBox) -> Rgb {
    let points = edge_pixels(image, bbox);
    if points.is_empty() {
        return [255, 255, 255];
    }
    let mut sums = [0u64; 3];
    for rgb in &points {
        sums[0] += rgb[0] as u64;
        sums[1] += rgb[1] as u64;
        sums[2] += rgb[2] as u64;
    }
    let n = points.len();
    [
        truncate_div(sums[0], n),
        truncate_div(sums[1], n),
        truncate_div(sums[2], n),
    ]
}

/// 整數平均後向零截斷，對應 `np.mean(...).astype(np.uint8)`。
#[inline]
fn truncate_div(sum: u64, count: usize) -> u8 {
    if count == 0 {
        return 0;
    }
    let value = sum as f64 / count as f64;
    value.clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_region_has_zero_diversity() {
        let img = RgbImage::filled(20, 20, [10, 20, 30]);
        let (diversity, color) = edge_diversity(&img, BBox::new(2, 2, 18, 18), 15);
        assert_eq!(diversity, 0.0);
        assert_eq!(color, [10, 20, 30]);
    }

    #[test]
    fn two_color_edges_split_evenly() {
        // 上緣白、下緣黑、左右各半 → 四個邊各 10 個點，白 20 黑 20
        let mut img = RgbImage::filled(12, 12, [0, 0, 0]);
        for x in 0..12 {
            img.set_pixel(x, 2, [255, 255, 255]);
            img.set_pixel(x, 9, [0, 0, 0]);
        }
        for y in 2..10 {
            img.set_pixel(2, y, [255, 255, 255]);
            img.set_pixel(9, y, [0, 0, 0]);
        }
        let (diversity, _) = edge_diversity(&img, BBox::new(2, 2, 10, 10), 15);
        assert!((diversity - 0.5).abs() < 1e-12, "實際 {diversity}");
    }

    #[test]
    fn tolerance_buckets_close_colors_together() {
        let mut img = RgbImage::filled(10, 10, [100, 100, 100]);
        for x in 0..10 {
            img.set_pixel(x, 1, [104, 100, 100]);
        }
        // tolerance 15 → 100 與 104 同桶（100//15=6, 104//15=6）
        let (tight, _) = edge_diversity(&img, BBox::new(1, 1, 9, 9), 15);
        // tolerance 4 → 不同桶（100//4=25, 104//4=26）
        let (loose, _) = edge_diversity(&img, BBox::new(1, 1, 9, 9), 4);
        assert!(tight < loose, "放寬容差應降低多樣性：{tight} vs {loose}");
    }

    #[test]
    fn empty_bbox_returns_white() {
        let img = RgbImage::filled(5, 5, [1, 2, 3]);
        let (diversity, color) = edge_diversity(&img, BBox::new(2, 2, 2, 2), 15);
        assert_eq!(diversity, 1.0);
        assert_eq!(color, [255, 255, 255]);
    }
}
