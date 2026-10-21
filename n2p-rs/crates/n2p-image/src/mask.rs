//! 布林遮罩（要修補的區域）。

use n2p_core::BBox;

/// 單通道布林遮罩，尺寸與來源影像一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    bits: Vec<bool>,
}

impl Mask {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            bits: vec![false; (width as usize) * (height as usize)],
        }
    }

    /// 由矩形建立遮罩。
    pub fn from_bbox(width: u32, height: u32, bbox: BBox) -> Self {
        let mut mask = Self::new(width, height);
        let bbox = bbox.clamp(width as i64, height as i64);
        for y in bbox.top.max(0)..bbox.bottom.max(0) {
            for x in bbox.left.max(0)..bbox.right.max(0) {
                mask.set(x as u32, y as u32, true);
            }
        }
        mask
    }

    #[inline]
    pub fn get(&self, x: u32, y: u32) -> bool {
        self.bits[(y as usize) * (self.width as usize) + (x as usize)]
    }

    #[inline]
    pub fn set(&mut self, x: u32, y: u32, value: bool) {
        self.bits[(y as usize) * (self.width as usize) + (x as usize)] = value;
    }

    pub fn count(&self) -> usize {
        self.bits.iter().filter(|b| **b).count()
    }

    pub fn is_empty(&self) -> bool {
        !self.bits.iter().any(|b| *b)
    }

    pub fn bits(&self) -> &[bool] {
        &self.bits
    }

    /// 遮罩的外接框；空遮罩回 `None`。
    pub fn bbox(&self) -> Option<BBox> {
        let mut min_x = u32::MAX;
        let mut min_y = u32::MAX;
        let mut max_x = 0u32;
        let mut max_y = 0u32;
        let mut found = false;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) {
                    found = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        found.then(|| {
            BBox::new(
                min_x as i64,
                min_y as i64,
                max_x as i64 + 1,
                max_y as i64 + 1,
            )
        })
    }

    /// 以方形核膨脹 `radius` 像素。
    ///
    /// 用途：文字的光暈／外框本身也是「文字效果」，只擦筆畫會留下一圈亮邊；
    /// 交給 LaMa 重建時把遮罩放大一點，讓它連光暈一起吃掉。
    ///
    /// 方形核可拆成「先橫向、再縱向」兩次一維膨脹，每次只掃一遍：O(像素數)，
    /// 與半徑無關（逐點畫方塊是 O(像素數 × 半徑²)，35 px 半徑的整頁遮罩要數秒）。
    pub fn dilate(&mut self, radius: u32) {
        if radius == 0 || self.is_empty() {
            return;
        }
        let (width, height) = (self.width as usize, self.height as usize);
        let radius = radius as usize;
        let mut rows = vec![false; self.bits.len()];
        for y in 0..height {
            dilate_line(&self.bits, &mut rows, y * width, 1, width, radius);
        }
        let mut out = vec![false; self.bits.len()];
        for x in 0..width {
            dilate_line(&rows, &mut out, x, width, height, radius);
        }
        self.bits = out;
    }

    /// 同 [`Mask::dilate`]，但只在 `limit` 範圍內生效。
    ///
    /// 用途：擦文字光暈時不要波及旁邊的裝飾（CH9 第 3 頁的引號就被方形膨脹
    /// 吃掉一角）。結果等於「整張膨脹」再與 `limit` 取交集（`limit` 外連原本的
    /// 像素也會清掉）。
    pub fn dilate_within(&mut self, radius: u32, limit: BBox) {
        if radius == 0 || self.is_empty() {
            return;
        }
        self.dilate(radius);
        let limit = limit.clamp(self.width as i64, self.height as i64);
        let width = self.width as usize;
        for y in 0..self.height as i64 {
            let inside_rows = y >= limit.top && y < limit.bottom;
            for x in 0..self.width as i64 {
                if !(inside_rows && x >= limit.left && x < limit.right) {
                    self.bits[y as usize * width + x as usize] = false;
                }
            }
        }
    }

    /// 把外接框往外擴 `pad` 像素並夾到影像範圍內。
    ///
    /// 對應 numpy 版的 `y1 = max(0, rows.min()-pad)`、`y2 = min(H, rows.max()+pad+1)`。
    pub fn padded_bbox(&self, pad: i64) -> Option<BBox> {
        let bbox = self.bbox()?;
        Some(
            BBox::new(
                bbox.left - pad,
                bbox.top - pad,
                bbox.right + pad,
                bbox.bottom + pad,
            )
            .clamp(self.width as i64, self.height as i64),
        )
    }
}

/// 一維膨脹：`dst[i] = src` 在 `[i - radius, i + radius]` 內有任一個 `true`。
/// 第 i 個元素位於 `start + i * stride`。先記住左邊最近的 `true`，再從右往左補上右邊的。
fn dilate_line(
    src: &[bool],
    dst: &mut [bool],
    start: usize,
    stride: usize,
    len: usize,
    radius: usize,
) {
    let mut last: Option<usize> = None;
    for i in 0..len {
        if src[start + i * stride] {
            last = Some(i);
        }
        dst[start + i * stride] = last.is_some_and(|j| i - j <= radius);
    }
    let mut next: Option<usize> = None;
    for i in (0..len).rev() {
        if src[start + i * stride] {
            next = Some(i);
        }
        if next.is_some_and(|j| j - i <= radius) {
            dst[start + i * stride] = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 原本逐點畫方塊的實作，當作參考答案。
    fn dilate_reference(mask: &Mask, radius: i64, limit: Option<BBox>) -> Vec<bool> {
        let limit = limit.unwrap_or(BBox::new(0, 0, mask.width as i64, mask.height as i64));
        let mut out = vec![false; mask.bits.len()];
        for y in 0..mask.height as i64 {
            for x in 0..mask.width as i64 {
                if !mask.get(x as u32, y as u32) {
                    continue;
                }
                let y0 = (y - radius).max(limit.top).max(0);
                let y1 = (y + radius)
                    .min(limit.bottom - 1)
                    .min(mask.height as i64 - 1);
                let x0 = (x - radius).max(limit.left).max(0);
                let x1 = (x + radius).min(limit.right - 1).min(mask.width as i64 - 1);
                for ny in y0..=y1.max(y0 - 1) {
                    for nx in x0..=x1.max(x0 - 1) {
                        if nx >= 0 && ny >= 0 {
                            out[ny as usize * mask.width as usize + nx as usize] = true;
                        }
                    }
                }
            }
        }
        out
    }

    fn random_mask(width: u32, height: u32, seed: u32, density: u32) -> Mask {
        let mut state = seed.max(1);
        let mut mask = Mask::new(width, height);
        for y in 0..height {
            for x in 0..width {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                if state % 1000 < density {
                    mask.set(x, y, true);
                }
            }
        }
        mask
    }

    #[test]
    fn separable_dilate_matches_drawing_squares() {
        for (seed, (w, h)) in [(1, (37, 23)), (2, (1, 9)), (3, (64, 1)), (4, (50, 50))] {
            for density in [3, 40, 500] {
                for radius in [1, 2, 5, 35, 80] {
                    let mask = random_mask(w, h, seed * 100 + density, density);
                    let mut fast = mask.clone();
                    fast.dilate(radius);
                    assert_eq!(
                        fast.bits,
                        dilate_reference(&mask, radius as i64, None),
                        "{w}x{h} 密度 {density} 半徑 {radius}"
                    );
                }
            }
        }
    }

    #[test]
    fn dilate_within_matches_drawing_clipped_squares() {
        let limits = [
            BBox::new(5, 3, 30, 20),
            BBox::new(-10, -10, 100, 100),
            BBox::new(20, 10, 21, 11),
            BBox::new(30, 30, 10, 10),
        ];
        for (i, limit) in limits.into_iter().enumerate() {
            for radius in [1, 4, 12] {
                let mask = random_mask(37, 23, 7 + i as u32, 30);
                let mut fast = mask.clone();
                fast.dilate_within(radius, limit);
                assert_eq!(
                    fast.bits,
                    dilate_reference(&mask, radius as i64, Some(limit)),
                    "limit {limit:?} 半徑 {radius}"
                );
            }
        }
    }

    #[test]
    fn bbox_and_padding() {
        let mask = Mask::from_bbox(20, 10, BBox::new(5, 3, 8, 6));
        assert_eq!(mask.count(), 9);
        assert_eq!(mask.bbox(), Some(BBox::new(5, 3, 8, 6)));
        assert_eq!(mask.padded_bbox(2), Some(BBox::new(3, 1, 10, 8)));
        // 夾到邊界
        assert_eq!(mask.padded_bbox(100), Some(BBox::new(0, 0, 20, 10)));
    }

    #[test]
    fn empty_mask() {
        let mask = Mask::new(4, 4);
        assert!(mask.is_empty());
        assert_eq!(mask.bbox(), None);
        assert_eq!(mask.padded_bbox(2), None);
    }
}
