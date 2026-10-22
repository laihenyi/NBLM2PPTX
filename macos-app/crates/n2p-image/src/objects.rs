//! 平坦底色頁面的獨立物件偵測：找出「與底色不同」的連通區塊。
//!
//! 輸入應是**已擦掉文字**的頁面影像，所以不必再處理文字重疊 —— 文字另成文字框，
//! 物件移動時文字不會跟著動。
//!
//! 只在底色佔頁面大宗（色塊、緩漸層）時有效。攝影或強漸層背景沒有「底色」可言，
//! 回傳空清單，由呼叫端決定要不要改用其他偵測器。

use n2p_core::{BBox, RgbImage};

use crate::mask::Mask;

/// 偵測參數。
#[derive(Debug, Clone, Copy)]
pub struct ObjectOptions {
    /// 與底色任一色版差距超過此值才算「不同」（0–255）。
    pub tolerance: u8,
    /// 底色像素至少要佔頁面的比例；低於此值視為非平坦背景。
    pub min_background_fraction: f64,
    /// 連通區塊面積下限（佔頁面比例）。
    pub min_area_fraction: f64,
    /// 外接框寬高同時超過頁面此比例的區塊視為頁面邊框，不當物件。
    pub max_extent_fraction: f64,
    /// 先做閉運算再膨脹的半徑（像素，依頁面短邊的比例換算後的下限）。
    pub join_radius: u32,
}

impl Default for ObjectOptions {
    fn default() -> Self {
        Self {
            tolerance: 18,
            min_background_fraction: 0.35,
            min_area_fraction: 0.0008,
            max_extent_fraction: 0.9,
            join_radius: 3,
        }
    }
}

/// 一個偵測到的物件。
#[derive(Debug, Clone)]
pub struct DetectedObject {
    /// 外接框（影像座標）。
    pub bbox: BBox,
    /// 物件像素遮罩（影像座標，尺寸同來源影像）。
    pub mask: Mask,
}

/// 估計頁面底色；底色佔比不足時回 `None`。
pub fn flat_background_color(image: &RgbImage, options: &ObjectOptions) -> Option<[u8; 3]> {
    let total = image.pixel_count();
    if total == 0 {
        return None;
    }
    // 5 bit 量化直方圖取眾數，再用該格內像素的平均當底色。
    let mut counts = vec![0u32; 32 * 32 * 32];
    let mut sums = vec![[0u64; 3]; 32 * 32 * 32];
    for pixel in image.data.chunks_exact(3) {
        let key = ((pixel[0] as usize >> 3) << 10)
            | ((pixel[1] as usize >> 3) << 5)
            | (pixel[2] as usize >> 3);
        counts[key] += 1;
        for channel in 0..3 {
            sums[key][channel] += pixel[channel] as u64;
        }
    }
    let (mode, _) = counts.iter().enumerate().max_by_key(|(_, count)| **count)?;
    let n = counts[mode] as u64;
    let color = [
        (sums[mode][0] / n) as u8,
        (sums[mode][1] / n) as u8,
        (sums[mode][2] / n) as u8,
    ];
    let near = image
        .data
        .chunks_exact(3)
        .filter(|p| is_near(p, color, options.tolerance))
        .count();
    (near as f64 / total as f64 >= options.min_background_fraction).then_some(color)
}

#[inline]
fn is_near(pixel: &[u8], color: [u8; 3], tolerance: u8) -> bool {
    (0..3).all(|c| pixel[c].abs_diff(color[c]) <= tolerance)
}

/// 在平坦底色頁面上找獨立物件。底色不平坦時回傳空清單。
pub fn find_flat_background_objects(
    image: &RgbImage,
    options: &ObjectOptions,
) -> Vec<DetectedObject> {
    let Some(background) = flat_background_color(image, options) else {
        return Vec::new();
    };
    let (w, h) = (image.width as usize, image.height as usize);

    let mut different: Vec<bool> = image
        .data
        .chunks_exact(3)
        .map(|p| !is_near(p, background, options.tolerance))
        .collect();

    // 先閉運算把斷裂的細線接起來，再膨脹一圈把鄰近零件（羅盤的齒輪與外環）併成一件。
    let radius = options.join_radius.max(1) as usize;
    different = erode(&dilate(&different, w, h, radius), w, h, radius);
    let joined = dilate(&different, w, h, radius + 1);

    let page_area = (w * h) as f64;
    let min_area = (page_area * options.min_area_fraction).max(16.0);
    let mut visited = vec![false; w * h];
    let mut objects = Vec::new();
    let mut stack = Vec::new();

    for start in 0..w * h {
        if !joined[start] || visited[start] {
            continue;
        }
        let mut members = Vec::new();
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (usize::MAX, usize::MAX, 0, 0);
        stack.push(start);
        visited[start] = true;
        while let Some(index) = stack.pop() {
            members.push(index);
            let (x, y) = (index % w, index / w);
            // 外接框只算真正不同於底色的像素，不含為併件而膨脹出來的邊。
            if different[index] {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let neighbour = ny as usize * w + nx as usize;
                    if joined[neighbour] && !visited[neighbour] {
                        visited[neighbour] = true;
                        stack.push(neighbour);
                    }
                }
            }
        }

        let area = members.iter().filter(|i| different[**i]).count() as f64;
        if area < min_area || min_x == usize::MAX {
            continue;
        }
        let (box_w, box_h) = ((max_x - min_x + 1) as f64, (max_y - min_y + 1) as f64);
        if box_w >= w as f64 * options.max_extent_fraction
            && box_h >= h as f64 * options.max_extent_fraction
        {
            continue;
        }

        // 遮罩只收「真的與底色不同」的像素，不收為了併件而膨脹出來的部分。
        let mut mask = Mask::new(image.width, image.height);
        for index in members {
            if different[index] {
                mask.set((index % w) as u32, (index / w) as u32, true);
            }
        }
        objects.push(DetectedObject {
            bbox: BBox::new(
                min_x as i64,
                min_y as i64,
                max_x as i64 + 1,
                max_y as i64 + 1,
            ),
            mask,
        });
    }
    objects
}

/// 方形核膨脹（可分離，O(n·r)）。
fn dilate(source: &[bool], w: usize, h: usize, radius: usize) -> Vec<bool> {
    sweep(source, w, h, radius, true)
}

/// 方形核侵蝕。
fn erode(source: &[bool], w: usize, h: usize, radius: usize) -> Vec<bool> {
    sweep(source, w, h, radius, false)
}

fn sweep(source: &[bool], w: usize, h: usize, radius: usize, grow: bool) -> Vec<bool> {
    let pass = |input: &[bool], horizontal: bool| -> Vec<bool> {
        let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
        let mut out = vec![!grow; input.len()];
        for o in 0..outer {
            for i in 0..inner {
                let lo = i.saturating_sub(radius);
                let hi = (i + radius).min(inner - 1);
                let index = |k: usize| if horizontal { o * w + k } else { k * w + o };
                let hit = if grow {
                    (lo..=hi).any(|k| input[index(k)])
                } else {
                    // 視窗在邊界處被截斷，所以貼邊的物件不會被侵蝕吃掉。
                    (lo..=hi).all(|k| input[index(k)])
                };
                out[index(i)] = hit;
            }
        }
        out
    };
    let horizontal = pass(source, true);
    pass(&horizontal, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(width: u32, height: u32, color: [u8; 3]) -> RgbImage {
        let mut image = RgbImage {
            width,
            height,
            data: vec![0; (width * height * 3) as usize],
        };
        for y in 0..height {
            for x in 0..width {
                image.set_pixel(x, y, color);
            }
        }
        image
    }

    fn fill(image: &mut RgbImage, bbox: BBox, color: [u8; 3]) {
        for y in bbox.top..bbox.bottom {
            for x in bbox.left..bbox.right {
                image.set_pixel(x as u32, y as u32, color);
            }
        }
    }

    #[test]
    fn two_separate_squares_become_two_objects() {
        let mut image = canvas(400, 300, [250, 240, 210]);
        fill(&mut image, BBox::new(40, 40, 120, 120), [180, 90, 40]);
        fill(&mut image, BBox::new(250, 150, 350, 250), [30, 60, 160]);
        let objects = find_flat_background_objects(&image, &ObjectOptions::default());
        assert_eq!(objects.len(), 2);
        let mut boxes: Vec<_> = objects.iter().map(|o| o.bbox).collect();
        boxes.sort_by_key(|b| b.left);
        assert_eq!(boxes[0], BBox::new(40, 40, 120, 120));
        assert_eq!(boxes[1], BBox::new(250, 150, 350, 250));
    }

    #[test]
    fn mask_excludes_background_pixels() {
        let mut image = canvas(200, 200, [250, 240, 210]);
        fill(&mut image, BBox::new(50, 50, 100, 100), [10, 10, 10]);
        let objects = find_flat_background_objects(&image, &ObjectOptions::default());
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].mask.count(), 50 * 50);
        assert!(!objects[0].mask.get(10, 10));
    }

    #[test]
    fn textured_background_yields_nothing() {
        let mut image = canvas(200, 200, [0, 0, 0]);
        for y in 0..200u32 {
            for x in 0..200u32 {
                let v = ((x * 7 + y * 13) % 256) as u8;
                image.set_pixel(x, y, [v, 255 - v, v / 2]);
            }
        }
        assert!(find_flat_background_objects(&image, &ObjectOptions::default()).is_empty());
    }

    #[test]
    fn page_border_and_specks_are_ignored() {
        let mut image = canvas(400, 300, [250, 240, 210]);
        // 頁面邊框（外框線）
        for t in 0..4 {
            fill(&mut image, BBox::new(5 + t, 5, 6 + t, 295), [100, 60, 30]);
            fill(
                &mut image,
                BBox::new(394 - t, 5, 395 - t, 295),
                [100, 60, 30],
            );
            fill(&mut image, BBox::new(5, 5 + t, 395, 6 + t), [100, 60, 30]);
            fill(
                &mut image,
                BBox::new(5, 294 - t, 395, 295 - t),
                [100, 60, 30],
            );
        }
        // 極小雜點
        fill(&mut image, BBox::new(200, 150, 203, 153), [0, 0, 0]);
        assert!(find_flat_background_objects(&image, &ObjectOptions::default()).is_empty());
    }

    #[test]
    fn nearby_parts_merge_into_one_object() {
        let mut image = canvas(400, 300, [250, 240, 210]);
        fill(&mut image, BBox::new(100, 100, 140, 140), [180, 90, 40]);
        fill(&mut image, BBox::new(144, 100, 184, 140), [180, 90, 40]);
        let objects = find_flat_background_objects(&image, &ObjectOptions::default());
        assert_eq!(objects.len(), 1);
    }
}
