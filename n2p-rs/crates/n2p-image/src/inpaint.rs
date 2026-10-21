//! 修補演算法。
//!
//! `manual` 與 `onion` 是 `inpaint_methods.py` 的逐位元移植：**兩者都是 Jacobi 迭代**
//! （先算完整張更新量，再套用），不是 Gauss-Seidel。若改成原地更新，結果會不同，
//! 因此這裡刻意用暫存緩衝。
//!
//! `idw` 與 `harmonic` 則是替代品：
//!
//! * 原版 `griddata` 需要 SciPy 的 `griddata`（Delaunay 線性插值），Rust 生態沒有等價物，
//!   改用反距離加權（IDW）。視覺上與原版接近，但不會逐位元相同。
//! * 原版 `skimage` 是 `inpaint_biharmonic`，需要解稀疏線性系統；這裡改用
//!   Laplace 迭代（調和插值），是雙調和的平滑近似。

use n2p_core::{BBox, RgbImage};

use crate::mask::Mask;

/// 從影像取出一塊區域，轉成 `f32` 的 RGB 緩衝（逐像素 triples）。
struct Region {
    bbox: BBox,
    width: usize,
    height: usize,
    data: Vec<[f32; 3]>,
}

impl Region {
    fn extract(image: &RgbImage, bbox: BBox) -> Self {
        let width = bbox.width().max(0) as usize;
        let height = bbox.height().max(0) as usize;
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let rgb = image.pixel(bbox.left as u32 + x as u32, bbox.top as u32 + y as u32);
                data.push([rgb[0] as f32, rgb[1] as f32, rgb[2] as f32]);
            }
        }
        Self {
            bbox,
            width,
            height,
            data,
        }
    }

    #[inline]
    fn index(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    /// 寫回影像，並套用 `clip(0,255)` 後轉 `u8`（對應 `np.clip(...).astype(np.uint8)`）。
    fn write_back(&self, image: &mut RgbImage) {
        for y in 0..self.height {
            for x in 0..self.width {
                let value = self.data[self.index(x, y)];
                image.set_pixel(
                    self.bbox.left as u32 + x as u32,
                    self.bbox.top as u32 + y as u32,
                    [
                        value[0].clamp(0.0, 255.0) as u8,
                        value[1].clamp(0.0, 255.0) as u8,
                        value[2].clamp(0.0, 255.0) as u8,
                    ],
                );
            }
        }
    }
}

/// 取出遮罩在區域內的子遮罩。
fn mask_in_region(mask: &Mask, bbox: BBox) -> Vec<bool> {
    let width = bbox.width().max(0) as usize;
    let height = bbox.height().max(0) as usize;
    let mut out = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            out.push(mask.get(bbox.left as u32 + x as u32, bbox.top as u32 + y as u32));
        }
    }
    out
}

/// 遮罩填色 + 拉普拉斯平滑迭代（`inpaint_manual`）。
///
/// 對應 Python：
/// ```text
/// img_crop[mask_crop] = fill_color
/// for _ in range(max_iter):
///     avg = (up + down + left + right) / 4.0
///     copyto(center, avg, where=mask)
/// ```
pub fn manual(image: &RgbImage, mask: &Mask, fill_color: [u8; 3], max_iter: usize) -> RgbImage {
    let Some(bbox) = mask.padded_bbox(2) else {
        return image.clone();
    };
    let mut out = image.clone();
    let mut region = Region::extract(image, bbox);
    let region_mask = mask_in_region(mask, bbox);

    let fill = [
        fill_color[0] as f32,
        fill_color[1] as f32,
        fill_color[2] as f32,
    ];
    for (index, inside) in region_mask.iter().enumerate() {
        if *inside {
            region.data[index] = fill;
        }
    }

    if max_iter > 0 && region.width >= 3 && region.height >= 3 {
        let mut scratch = region.data.clone();
        for _ in 0..max_iter {
            // Jacobi：先從「當前」狀態算出全部新值。
            scratch.copy_from_slice(&region.data);
            for y in 1..region.height - 1 {
                for x in 1..region.width - 1 {
                    if !region_mask[region.index(x, y)] {
                        continue;
                    }
                    // 加法順序與 numpy 的 `up + down + left + right` 一致。
                    let up = region.data[region.index(x, y - 1)];
                    let down = region.data[region.index(x, y + 1)];
                    let left = region.data[region.index(x - 1, y)];
                    let right = region.data[region.index(x + 1, y)];
                    scratch[region.index(x, y)] = [
                        ((up[0] + down[0]) + left[0] + right[0]) / 4.0,
                        ((up[1] + down[1]) + left[1] + right[1]) / 4.0,
                        ((up[2] + down[2]) + left[2] + right[2]) / 4.0,
                    ];
                }
            }
            std::mem::swap(&mut region.data, &mut scratch);
        }
    }

    region.write_back(&mut out);
    out
}

/// 由外向内逐層修補（`inpaint_numpy_onion`）。
pub fn onion(image: &RgbImage, mask: &Mask) -> RgbImage {
    let Some(bbox) = mask.padded_bbox(2) else {
        return image.clone();
    };
    let mut out = image.clone();
    let mut region = Region::extract(image, bbox);
    let mut remaining = mask_in_region(mask, bbox);

    let (width, height) = (region.width, region.height);
    if width == 0 || height == 0 {
        return out;
    }

    // `max_steps = max(img_crop.shape)`
    let max_steps = width.max(height).max(3);
    const OFFSETS: [(i32, i32); 8] = [
        (-1, -1),
        (-1, 0),
        (-1, 1),
        (0, -1),
        (0, 1),
        (1, -1),
        (1, 0),
        (1, 1),
    ];

    for _ in 0..max_steps {
        if !remaining.iter().any(|b| *b) {
            break;
        }

        // 邊緣層 = 遮罩 AND NOT 侵蝕後的遮罩
        // 侵蝕用的 pad 值為 1（越界視為「仍待修」），因此核心要求自身與上下左右皆為 1。
        let at = |x: i32, y: i32| -> bool {
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                true
            } else {
                remaining[y as usize * width + x as usize]
            }
        };
        let mut border_mask = vec![false; width * height];
        let mut border_count = 0usize;
        for y in 0..height {
            for x in 0..width {
                if !remaining[y * width + x] {
                    continue;
                }
                let (xi, yi) = (x as i32, y as i32);
                let core = at(xi, yi - 1) && at(xi, yi + 1) && at(xi - 1, yi) && at(xi + 1, yi);
                if !core {
                    border_mask[y * width + x] = true;
                    border_count += 1;
                }
            }
        }

        if border_count == 0 {
            // 兜底：剩下的直接填已知像素的均值
            let mut sums = [0f64; 3];
            let mut count = 0usize;
            for (index, inside) in remaining.iter().enumerate() {
                if !*inside {
                    sums[0] += region.data[index][0] as f64;
                    sums[1] += region.data[index][1] as f64;
                    sums[2] += region.data[index][2] as f64;
                    count += 1;
                }
            }
            let fill = if count > 0 {
                [
                    (sums[0] / count as f64) as f32,
                    (sums[1] / count as f64) as f32,
                    (sums[2] / count as f64) as f32,
                ]
            } else {
                [255.0, 255.0, 255.0]
            };
            for (index, inside) in remaining.iter().enumerate() {
                if *inside {
                    region.data[index] = fill;
                }
            }
            break;
        }

        // 3×3 鄰域中「已知像素」的加權平均（越界以 0 計）
        let mut updates: Vec<(usize, [f32; 3])> = Vec::with_capacity(border_count);
        for y in 0..height {
            for x in 0..width {
                let index = y * width + x;
                if !border_mask[index] {
                    continue;
                }
                let mut sum = [0f32; 3];
                let mut weight = 0f32;
                for (dy, dx) in OFFSETS {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    let neighbour = ny as usize * width + nx as usize;
                    if remaining[neighbour] {
                        continue;
                    }
                    let value = region.data[neighbour];
                    sum[0] += value[0];
                    sum[1] += value[1];
                    sum[2] += value[2];
                    weight += 1.0;
                }
                let divisor = if weight == 0.0 { 1.0 } else { weight };
                updates.push((
                    index,
                    [sum[0] / divisor, sum[1] / divisor, sum[2] / divisor],
                ));
            }
        }
        for (index, value) in updates {
            region.data[index] = value;
            remaining[index] = false;
        }
    }

    region.write_back(&mut out);
    out
}

/// 反距離加權插值（取代 SciPy 的 `griddata`）。
///
/// `radius` 內所有已知像素以 `1/d²` 加權；`radius` 外不參與。`stride` 用來抽樣，
/// 控制成本。
pub fn idw(image: &RgbImage, mask: &Mask, radius: u32, stride: u32) -> RgbImage {
    let Some(bbox) = mask.padded_bbox(radius as i64) else {
        return image.clone();
    };
    let mut out = image.clone();
    let mut region = Region::extract(image, bbox);
    let region_mask = mask_in_region(mask, bbox);
    let radius = radius.max(1) as i64;
    let stride = stride.max(1) as usize;

    // 先蒐集已知像素的座標，避免對每個目標像素掃全區。
    let mut known: Vec<(usize, usize, [f32; 3])> = Vec::new();
    for y in (0..region.height).step_by(stride) {
        for x in (0..region.width).step_by(stride) {
            if !region_mask[region.index(x, y)] {
                known.push((x, y, region.data[region.index(x, y)]));
            }
        }
    }
    if known.is_empty() {
        return out;
    }

    let mut updates: Vec<(usize, [f32; 3])> = Vec::new();
    for y in 0..region.height {
        for x in 0..region.width {
            if !region_mask[region.index(x, y)] {
                continue;
            }
            let mut sum = [0f64; 3];
            let mut weight_sum = 0f64;
            for (kx, ky, value) in &known {
                let dx = x as i64 - *kx as i64;
                let dy = y as i64 - *ky as i64;
                if dx.abs() > radius || dy.abs() > radius {
                    continue;
                }
                let distance_sq = (dx * dx + dy * dy) as f64;
                if distance_sq == 0.0 {
                    continue;
                }
                let weight = 1.0 / distance_sq;
                sum[0] += value[0] as f64 * weight;
                sum[1] += value[1] as f64 * weight;
                sum[2] += value[2] as f64 * weight;
                weight_sum += weight;
            }
            if weight_sum > 0.0 {
                updates.push((
                    region.index(x, y),
                    [
                        (sum[0] / weight_sum) as f32,
                        (sum[1] / weight_sum) as f32,
                        (sum[2] / weight_sum) as f32,
                    ],
                ));
            }
        }
    }
    for (index, value) in updates {
        region.data[index] = value;
    }

    region.write_back(&mut out);
    out
}

/// 調和插值（Laplace 迭代），作為 `inpaint_biharmonic` 的近似。
///
/// 邊界固定為已知像素，只對遮罩內部反覆平均，直到收斂或達到 `max_iter`。
pub fn harmonic(image: &RgbImage, mask: &Mask, max_iter: usize) -> RgbImage {
    let Some(bbox) = mask.padded_bbox(2) else {
        return image.clone();
    };
    let mut out = image.clone();
    let mut region = Region::extract(image, bbox);
    let region_mask = mask_in_region(mask, bbox);

    if region.width < 3 || region.height < 3 {
        return out;
    }

    for _ in 0..max_iter {
        let mut changed = 0f32;
        let mut scratch = region.data.clone();
        for y in 1..region.height - 1 {
            for x in 1..region.width - 1 {
                if !region_mask[region.index(x, y)] {
                    continue;
                }
                let up = region.data[region.index(x, y - 1)];
                let down = region.data[region.index(x, y + 1)];
                let left = region.data[region.index(x - 1, y)];
                let right = region.data[region.index(x + 1, y)];
                let next = [
                    ((up[0] + down[0]) + left[0] + right[0]) / 4.0,
                    ((up[1] + down[1]) + left[1] + right[1]) / 4.0,
                    ((up[2] + down[2]) + left[2] + right[2]) / 4.0,
                ];
                let current = region.data[region.index(x, y)];
                changed = changed
                    .max((next[0] - current[0]).abs())
                    .max((next[1] - current[1]).abs())
                    .max((next[2] - current[2]).abs());
                scratch[region.index(x, y)] = next;
            }
        }
        std::mem::swap(&mut region.data, &mut scratch);
        if changed < 0.01 {
            break;
        }
    }

    region.write_back(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: u32, height: u32) -> RgbImage {
        let mut img = RgbImage::new(width, height);
        for y in 0..height {
            let v = (y * 255 / height.max(1)) as u8;
            for x in 0..width {
                img.set_pixel(x, y, [v, v, v]);
            }
        }
        img
    }

    #[test]
    fn manual_keeps_pixels_outside_mask() {
        let img = gradient(40, 30);
        let mask = Mask::from_bbox(40, 30, BBox::new(10, 10, 20, 20));
        let out = manual(&img, &mask, [0, 0, 0], 20);
        for y in 0..30 {
            for x in 0..40 {
                if !(10..20).contains(&x) || !(10..20).contains(&y) {
                    assert_eq!(out.pixel(x, y), img.pixel(x, y), "({x},{y}) 不應被改動");
                }
            }
        }
    }

    #[test]
    fn manual_smooths_toward_neighbourhood() {
        let img = gradient(40, 30);
        let mask = Mask::from_bbox(40, 30, BBox::new(10, 10, 20, 20));
        let filled = manual(&img, &mask, [0, 0, 0], 0);
        let smoothed = manual(&img, &mask, [0, 0, 0], 200);
        // 迭代後遮罩中心應比純填黑色更接近周圍（漸層值約 110~130）
        let center = smoothed.pixel(15, 15)[0];
        assert!(center > 50, "平滑後應接近周圍灰階，實際 {center}");
        assert!(smoothed.pixel(15, 15)[0] > filled.pixel(15, 15)[0]);
    }

    #[test]
    fn onion_fills_entire_mask() {
        let img = gradient(40, 30);
        let mask = Mask::from_bbox(40, 30, BBox::new(10, 10, 20, 20));
        let out = onion(&img, &mask);
        // 遮罩內每個像素都應變成有意義的灰階（0 代表完全沒被處理）
        for y in 10..20 {
            for x in 10..20 {
                let v = out.pixel(x, y)[0];
                assert!(v > 0 && v < 255, "({x},{y}) 應被修補，實際 {v}");
            }
        }
    }

    #[test]
    fn idw_and_harmonic_produce_plausible_values() {
        let img = gradient(40, 30);
        let mask = Mask::from_bbox(40, 30, BBox::new(10, 10, 20, 20));
        for out in [idw(&img, &mask, 8, 2), harmonic(&img, &mask, 200)] {
            let v = out.pixel(15, 15)[0];
            assert!(v > 20 && v < 200, "值應落在周圍灰階範圍內，實際 {v}");
        }
    }

    #[test]
    fn empty_mask_is_a_noop() {
        let img = gradient(10, 10);
        let mask = Mask::new(10, 10);
        assert_eq!(manual(&img, &mask, [1, 2, 3], 10), img);
        assert_eq!(onion(&img, &mask), img);
        assert_eq!(idw(&img, &mask, 4, 1), img);
        assert_eq!(harmonic(&img, &mask, 10), img);
    }
}
