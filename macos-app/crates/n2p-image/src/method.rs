//! 修補方法的識別碼與中繼資料。
//!
//! `id` 與顯示名稱刻意與 Python 版 `INPAINT_METHODS` **完全一致**，
//! 這樣既有的 `config.json` 不需要遷移。

use serde::{Deserialize, Serialize};

/// 六種修補方法。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InpaintMethod {
    /// 智慧平滑（推薦）：填主色後做 100 次拉普拉斯平滑。
    #[default]
    BackgroundSmooth,
    /// 邊緣均值填充：填「邊緣平均色」後平滑。
    EdgeMeanSmooth,
    /// 極速純色填充：直接填主色，不平滑。
    Background,
    /// 逐層內縮修補（洋蔥皮）。
    Onion,
    /// 漸變過渡插值。Python 版用 SciPy `griddata`，Rust 版以 IDW 取代。
    Idw,
    /// 雙調和光影修補。Python 版用 `skimage.inpaint_biharmonic`，Rust 版以調和插值近似。
    Harmonic,
    /// 最近鄰接填充：每個遮罩像素取「最近的未遮罩像素」顏色，再平滑。
    ///
    /// 與 [`Self::BackgroundSmooth`] 的差別在於種子不是單一主色，而是逐像素
    /// 的局部背景色。文字壓在色帶交界（例如表格的黃色標題列）時，
    /// 單一主色會留下比色帶淡的方塊。
    NearestNeighbour,
}

impl InpaintMethod {
    /// 與 Python 版一致的穩定性識別碼。
    pub fn id(self) -> &'static str {
        match self {
            Self::BackgroundSmooth => "background_smooth",
            Self::EdgeMeanSmooth => "edge_mean_smooth",
            Self::Background => "background",
            Self::Onion => "onion",
            Self::Idw => "griddata",
            Self::Harmonic => "skimage",
            Self::NearestNeighbour => "nearest_neighbour",
        }
    }

    /// 顯示名稱（與 `INPAINT_METHODS` 的 `name` 相同，供 UI 使用）。
    pub fn display_name(self) -> &'static str {
        match self {
            Self::BackgroundSmooth => "智慧平滑（推薦）",
            Self::EdgeMeanSmooth => "邊緣均值填充",
            Self::Background => "極速純色填充",
            Self::Onion => "逐層內縮修補",
            Self::Idw => "漸變過渡插值",
            Self::Harmonic => "雙調和光影修補",
            Self::NearestNeighbour => "最近鄰接填充",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::BackgroundSmooth => "綜合效果最佳，適合大多數去除文字、水印的場景",
            Self::EdgeMeanSmooth => "取周圍像素平均色填充，適合純色或簡單背景",
            Self::Background => "直接填充單一背景色，僅適合極簡底色，速度最快",
            Self::Onion => "由外向內逐層修補，適合細長划痕或線條修復",
            Self::Idw => "以反距離加權近似曲面過渡，適合帶漸變的背景",
            Self::Harmonic => "以調和插值近似雙調和求解，較慢但光影較連續",
            Self::NearestNeighbour => "逐像素取最近的未遮罩像素當種子再平滑，適合色帶交界的文字",
        }
    }

    /// 是否與 Python 版逐位元等價。
    ///
    /// `Idw` 與 `Harmonic` 是替代演算法，只保證視覺接近。
    pub fn is_bit_exact_with_python(self) -> bool {
        matches!(
            self,
            Self::BackgroundSmooth | Self::EdgeMeanSmooth | Self::Background | Self::Onion
        )
    }

    /// 依識別碼或顯示名稱解析，退回預設值。
    ///
    /// 對應 Python 版的 `get_method_id`：未知字串一律回 `background_smooth`。
    pub fn parse(value: &str) -> Self {
        Self::try_parse(value).unwrap_or(Self::BackgroundSmooth)
    }

    pub fn try_parse(value: &str) -> Option<Self> {
        let value = value.trim();
        Self::all()
            .iter()
            .copied()
            .find(|m| m.id() == value || m.display_name() == value)
    }

    pub fn all() -> &'static [InpaintMethod] {
        &[
            Self::BackgroundSmooth,
            Self::EdgeMeanSmooth,
            Self::Background,
            Self::Onion,
            Self::Idw,
            Self::Harmonic,
        ]
    }
}

impl std::fmt::Display for InpaintMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_the_python_list() {
        let ids: Vec<&str> = InpaintMethod::all().iter().map(|m| m.id()).collect();
        assert_eq!(
            ids,
            vec![
                "background_smooth",
                "edge_mean_smooth",
                "background",
                "onion",
                "griddata",
                "skimage"
            ]
        );
    }

    #[test]
    fn parse_accepts_id_and_display_name() {
        assert_eq!(InpaintMethod::parse("onion"), InpaintMethod::Onion);
        assert_eq!(InpaintMethod::parse("逐層內縮修補"), InpaintMethod::Onion);
        // 未知值退回預設
        assert_eq!(
            InpaintMethod::parse("nope"),
            InpaintMethod::BackgroundSmooth
        );
        assert_eq!(InpaintMethod::try_parse("nope"), None);
    }

    #[test]
    fn bit_exact_flags() {
        assert!(InpaintMethod::BackgroundSmooth.is_bit_exact_with_python());
        assert!(!InpaintMethod::Idw.is_bit_exact_with_python());
        assert!(!InpaintMethod::Harmonic.is_bit_exact_with_python());
    }
}
