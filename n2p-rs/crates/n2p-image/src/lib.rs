//! # n2p-image
//!
//! 影像處理層，取代 Python 版的 `edge_diversity.py`、`inpaint_methods.py`、
//! `image_inpainter.py` 與 `pdf2png.py` 的寬屏化段落。
//!
//! | 模組 | Python 對應 |
//! | :--- | :--- |
//! | [`diversity`] | `edge_diversity.py` |
//! | [`inpaint`] | `inpaint_methods.py` |
//! | [`watermark`] | `image_inpainter.py` |
//! | [`transform`] | `make_wide_screen`／`make_data_wide_screen` |
//! | [`method`] | `INPAINT_METHODS` 清單 |
//!
//! ## 逐位元等價的範圍
//!
//! `background`、`background_smooth`、`edge_mean_smooth`、`onion` 四種與 numpy 版
//! 完全相同（含 f32 運算順序與 Jacobi 迭代語意），由 `tests/golden.rs` 逐像素驗證。
//!
//! `griddata` 與 `skimage` 是替代演算法：前者以 IDW 取代 SciPy 的 Delaunay 插值，
//! 後者以調和插值近似 `inpaint_biharmonic`。視覺接近但不保證逐位元相同，
//! 可用 [`InpaintMethod::is_bit_exact_with_python`] 查詢。

pub mod diversity;
pub mod inpaint;
pub mod lama;
pub mod mask;
pub mod method;
pub mod objects;
pub mod textmask;
pub mod transform;
pub mod watermark;

pub use diversity::{Rgb, edge_average_color, edge_diversity, edge_pixels};
pub use method::InpaintMethod;
pub use transform::{is_widescreen, to_widescreen};
pub use watermark::{
    inpaint_bbox_in_place, inpaint_mask_in_place, inpaint_region, inpaint_watermark, watermark_box,
};

pub use lama::{Lama, LamaError};
pub use mask::Mask;
pub use n2p_core::{BBox, RgbImage};
pub use objects::{
    DetectedObject, ObjectOptions, find_flat_background_objects, flat_background_color,
};
pub use textmask::{TextAnalysis, analyze as analyze_text_region};
