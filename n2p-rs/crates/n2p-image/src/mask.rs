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

    /// 把外接框往外擴 `pad` 像素並夾到影像範圍內。
    ///
    /// 對應 numpy 版的 `y1 = max(0, rows.min()-pad)`、`y2 = min(H, rows.max()+pad+1)`。
    /// 以方形核膨脹 `radius` 像素。
    ///
    /// 用途：文字的光暈／外框本身也是「文字效果」，只擦筆畫會留下一圈亮邊；
    /// 交給 LaMa 重建時把遮罩放大一點，讓它連光暈一起吃掉。
    pub fn dilate(&mut self, radius: u32) {
        if radius == 0 || self.is_empty() {
            return;
        }
        let radius = radius as i64;
        let mut out = vec![false; self.bits.len()];
        for y in 0..self.height {
            for x in 0..self.width {
                if !self.get(x, y) {
                    continue;
                }
                let (x, y) = (x as i64, y as i64);
                let y0 = (y - radius).max(0);
                let y1 = (y + radius).min(self.height as i64 - 1);
                let x0 = (x - radius).max(0);
                let x1 = (x + radius).min(self.width as i64 - 1);
                for ny in y0..=y1 {
                    for nx in x0..=x1 {
                        out[ny as usize * self.width as usize + nx as usize] = true;
                    }
                }
            }
        }
        self.bits = out;
    }

    /// 同 [`Mask::dilate`]，但只在 `limit` 範圍內生效。
    ///
    /// 用途：擦文字光暈時不要波及旁邊的裝飾（CH9 第 3 頁的引號就被方形膨脹
    /// 吃掉一角）。
    pub fn dilate_within(&mut self, radius: u32, limit: BBox) {
        if radius == 0 || self.is_empty() {
            return;
        }
        let radius = radius as i64;
        let mut out = vec![false; self.bits.len()];
        for y in 0..self.height {
            for x in 0..self.width {
                if !self.get(x, y) {
                    continue;
                }
                let (x, y) = (x as i64, y as i64);
                let y0 = (y - radius).max(limit.top).max(0);
                let y1 = (y + radius)
                    .min(limit.bottom - 1)
                    .min(self.height as i64 - 1);
                let x0 = (x - radius).max(limit.left).max(0);
                let x1 = (x + radius).min(limit.right - 1).min(self.width as i64 - 1);
                for ny in y0..=y1.max(y0 - 1) {
                    for nx in x0..=x1.max(x0 - 1) {
                        if nx < 0 || ny < 0 {
                            continue;
                        }
                        out[ny as usize * self.width as usize + nx as usize] = true;
                    }
                }
            }
        }
        self.bits = out;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

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
