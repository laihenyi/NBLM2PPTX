//! 單位換算。
//!
//! OOXML 內部座標用 EMU（English Metric Unit）。本專案上游全部用「點（pt）」，
//! 而 Spire.Presentation 的換算基準是 `1 pt = 12700 EMU`、字級 `1 pt = 100 centipoint`，
//! 與 ECMA-376 一致。此處照抄，避免逐位元偏差。

/// 每點對應的 EMU。
pub const EMU_PER_POINT: i64 = 12_700;
/// 每英吋對應的 EMU。
pub const EMU_PER_INCH: i64 = 914_400;

/// 點 → EMU（四捨五入）。
pub fn emu_from_pt(v: f64) -> i64 {
    (v * EMU_PER_POINT as f64).round() as i64
}

/// EMU → 點。
pub fn pt_from_emu(v: i64) -> f64 {
    v as f64 / EMU_PER_POINT as f64
}

/// 點 → 字級 centipoint（`sz="2400"` 這種）。
pub fn centipoint_from_pt(v: f64) -> i64 {
    (v * 100.0).round() as i64
}

/// 以點為單位的矩形，對應 Spire 的 `RectangleF.FromLTRB`。
///
/// `width()` / `height()` 採「兩端各自換算後相減」，與 Spire 的
/// `FromLTRB` 語意一致；若改成 `(r-l)*12700` 在非整數點上會差 1 EMU。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left_pt: f64,
    pub top_pt: f64,
    pub right_pt: f64,
    pub bottom_pt: f64,
}

impl Rect {
    pub fn ltrb(left_pt: f64, top_pt: f64, right_pt: f64, bottom_pt: f64) -> Self {
        Self {
            left_pt,
            top_pt,
            right_pt,
            bottom_pt,
        }
    }

    /// 供 `ShapeList_AppendShape` 用的等價建構子。
    pub fn width_pt(&self) -> f64 {
        self.right_pt - self.left_pt
    }

    pub fn height_pt(&self) -> f64 {
        self.bottom_pt - self.top_pt
    }

    /// `<a:off x= y=>` 的兩個值。
    pub fn emu_offset(&self) -> (i64, i64) {
        (emu_from_pt(self.left_pt), emu_from_pt(self.top_pt))
    }

    /// `<a:ext cx= cy=>` 的兩個值。
    pub fn emu_extent(&self) -> (i64, i64) {
        (
            emu_from_pt(self.right_pt) - emu_from_pt(self.left_pt),
            emu_from_pt(self.bottom_pt) - emu_from_pt(self.top_pt),
        )
    }

    /// 水平平移（`ppt_creater` 的 `delta_y` 對應的是垂直版）。
    pub fn translate_y(&self, dy_pt: f64) -> Self {
        Self {
            top_pt: self.top_pt + dy_pt,
            bottom_pt: self.bottom_pt + dy_pt,
            ..*self
        }
    }
}

/// 畫布尺寸，對應 `SlideSizeType`。
///
/// 三個具名值皆為 PowerPoint 官方數值，已對 `assets/probe-pptx/sz_*.pptx` 實測驗證。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideSize {
    /// 9144000 × 6858000 EMU（10in × 7.5in）
    Screen4x3,
    /// 9144000 × 5715000 EMU（10in × 6.25in）
    Screen16x10,
    /// 12192000 × 6858000 EMU（13.333in × 7.5in）
    Screen16x9,
    /// 自訂，直接給 EMU。
    Custom { cx: i64, cy: i64 },
}

impl SlideSize {
    /// 由 PDF 頁面寬高比自動挑選，對應 `setup_presentation` 的 `strategy == "diff"`。
    pub fn from_ratio(width_pt: f64, height_pt: f64) -> Self {
        let ratio = width_pt / height_pt;
        if ratio > 1.65 {
            Self::Screen16x9
        } else if ratio > 1.45 {
            Self::Screen16x10
        } else if ratio > 1.0 {
            Self::Screen4x3
        } else {
            let h = 720.0;
            let w = h * ratio;
            Self::Custom {
                cx: emu_from_pt(w),
                cy: emu_from_pt(h),
            }
        }
    }

    /// `(cx, cy)` EMU。
    pub fn emu(self) -> (i64, i64) {
        match self {
            Self::Screen4x3 => (9_144_000, 6_858_000),
            Self::Screen16x10 => (9_144_000, 5_715_000),
            Self::Screen16x9 => (12_192_000, 6_858_000),
            Self::Custom { cx, cy } => (cx, cy),
        }
    }

    /// `<p:sldSz @type>` 的值；`Custom` 不帶此屬性。
    pub fn type_attr(self) -> Option<&'static str> {
        match self {
            Self::Screen4x3 => Some("screen4x3"),
            Self::Screen16x10 => Some("screen16x10"),
            Self::Screen16x9 => Some("screen16x9"),
            Self::Custom { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_matches_spire_emu() {
        // 取自 assets/probe-pptx/1_text.pptx：FromLTRB(60,40,660,120)
        let r = Rect::ltrb(60.0, 40.0, 660.0, 120.0);
        assert_eq!(r.emu_offset(), (762_000, 508_000));
        assert_eq!(r.emu_extent(), (7_620_000, 1_016_000));
    }

    #[test]
    fn slide_size_matches_spire() {
        assert_eq!(SlideSize::Screen16x9.emu(), (12_192_000, 6_858_000));
        assert_eq!(SlideSize::Screen16x10.emu(), (9_144_000, 5_715_000));
        assert_eq!(SlideSize::Screen4x3.emu(), (9_144_000, 6_858_000));
    }

    #[test]
    fn ratio_picks_same_strategy_as_python() {
        assert_eq!(SlideSize::from_ratio(1000.0, 562.0), SlideSize::Screen16x9);
        assert_eq!(SlideSize::from_ratio(1000.0, 650.0), SlideSize::Screen16x10);
        assert_eq!(SlideSize::from_ratio(1000.0, 800.0), SlideSize::Screen4x3);
        assert!(matches!(
            SlideSize::from_ratio(600.0, 800.0),
            SlideSize::Custom { .. }
        ));
    }

    #[test]
    fn centipoints() {
        assert_eq!(centipoint_from_pt(24.0), 2400);
        assert_eq!(centipoint_from_pt(16.0), 1600);
    }
}
