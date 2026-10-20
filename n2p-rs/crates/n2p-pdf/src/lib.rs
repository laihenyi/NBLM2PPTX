//! PDF → PNG 點陣化，取代 Python 版的 PyMuPDF（`fitz`）。
//!
//! 只用得到三件事：頁數、頁面尺寸（點）、把某一頁點陣化成 PNG，
//! 因此直接架在 [`hayro`]（純 Rust PDF 光柵化器）上，不引入 C 依賴。
//!
//! [`hayro`]: https://crates.io/crates/hayro

use std::path::{Path, PathBuf};

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

/// 72 點 = 1 英吋，與 PDF 的預設座標系一致。
const POINTS_PER_INCH: f64 = 72.0;

/// 錯誤型別。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O 失敗：{0}")]
    Io(#[from] std::io::Error),

    #[error("PDF 載入失敗：{0}")]
    Load(String),

    #[error("頁索引 {index} 超出範圍（共 {count} 頁）")]
    PageOutOfRange { index: usize, count: usize },

    #[error("PNG 編碼失敗：{0}")]
    Encode(String),

    #[error("點陣化失敗：{0}")]
    Render(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub use n2p_core::RgbImage;

/// 一份已載入的 PDF。
pub struct PdfDoc {
    pdf: Pdf,
    bytes_len: usize,
}

impl PdfDoc {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(bytes)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let bytes_len = bytes.len();
        let pdf = Pdf::new(bytes).map_err(|e| Error::Load(format!("{e:?}")))?;
        Ok(Self { pdf, bytes_len })
    }

    pub fn bytes_len(&self) -> usize {
        self.bytes_len
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// 頁面尺寸（點）。
    ///
    /// 採 `render_dimensions()`：crop box 與 media box 的交集，且已套用頁面旋轉。
    /// 這正是點陣化時實際使用的基準，因此與 PyMuPDF 的 `page.rect` 語意一致。
    pub fn page_size_pt(&self, index: usize) -> Result<(f64, f64)> {
        let page = self.page(index)?;
        let (w, h) = page.render_dimensions();
        Ok((w as f64, h as f64))
    }

    /// 第一頁尺寸（點）。`setup_presentation(pdf_size)` 要的就是這個。
    pub fn first_page_size_pt(&self) -> Result<(f64, f64)> {
        self.page_size_pt(0)
    }

    /// 依 DPI 計算目標像素尺寸。
    ///
    /// **採向上取整，與 MuPDF（PyMuPDF）一致。** 這不是隨便選的：
    /// `image_inpainter.py` 裡的水印座標硬編碼以 `2867 x 1600` 為基準
    /// （對應本專案 `examples/floyd.pdf` 的 1376 x 768 pt @ 150 dpi）。
    /// 若改用截斷會得到 2866，雖然比例斷言的 1% 容差仍會通過，但會差一個像素。
    pub fn pixel_size(&self, index: usize, dpi: f64) -> Result<(u32, u32)> {
        let (w_pt, h_pt) = self.page_size_pt(index)?;
        if !dpi.is_finite() || dpi <= 0.0 {
            return Err(Error::Render(format!("DPI 無效：{dpi}")));
        }
        let factor = dpi / POINTS_PER_INCH;
        Ok(((w_pt * factor).ceil() as u32, (h_pt * factor).ceil() as u32))
    }

    /// 點陣化某一頁成 RGB。
    ///
    /// `dpi` 與 Python 版一致：`scale = dpi / 72`。
    pub fn render_rgb(&self, index: usize, dpi: f64) -> Result<RgbImage> {
        let page = self.page(index)?;
        let (target_w, target_h) = self.pixel_size(index, dpi)?;
        let (w_pt, h_pt) = self.page_size_pt(index)?;
        if target_w == 0 || target_h == 0 {
            return Err(Error::Render(format!("頁面尺寸無效：{w_pt} x {h_pt} pt")));
        }
        // 由「目標像素」反推縮放，確保輸出尺寸與 MuPDF 逐像素對齊。
        //
        // hayro 內部是 `(base * scale) as u16`（截斷），而 `base * (target/base)` 在 f32
        // 下會得到 2866.9999… → 截斷成 2866。因此把 scale 往上推到第一組
        // 「乘完剛好 ≥ target」的值，位移量遠小於一個像素。
        let x_scale = scale_for(w_pt, target_w)?;
        let y_scale = scale_for(h_pt, target_h)?;

        let cache = RenderCache::new();
        let pixmap = render(
            page,
            &cache,
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale,
                y_scale,
                bg_color: WHITE,
            },
        );

        let width = pixmap.width() as u32;
        let height = pixmap.height() as u32;
        let rgba = pixmap.data_as_u8_slice();
        let expected = (width as usize) * (height as usize) * 4;
        if rgba.len() != expected {
            return Err(Error::Render(format!(
                "點陣圖長度不符：{} vs {expected}",
                rgba.len()
            )));
        }

        // 背景是白、alpha 應為 255，因此 RGBA 的前三個通道即為直通色。
        // 若出現半透明像素（罕見），先還原預乘再加白底。
        let mut data = vec![0u8; (width as usize) * (height as usize) * 3];
        for (pixel, out) in rgba.chunks_exact(4).zip(data.chunks_exact_mut(3)) {
            let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
            if a == 255 {
                out[0] = r;
                out[1] = g;
                out[2] = b;
            } else {
                let alpha = a as f32 / 255.0;
                let blend = |c: u8| -> u8 {
                    let straight = if a == 0 { 0.0 } else { c as f32 / alpha };
                    (straight * alpha + 255.0 * (1.0 - alpha))
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                out[0] = blend(r);
                out[1] = blend(g);
                out[2] = blend(b);
            }
        }

        RgbImage::from_raw(width, height, data).map_err(|e| Error::Render(e.to_string()))
    }

    /// 點陣化某一頁成 PNG bytes。
    pub fn render_png(&self, index: usize, dpi: f64) -> Result<Vec<u8>> {
        self.render_rgb(index, dpi)?
            .to_png()
            .map_err(|e| Error::Encode(e.to_string()))
    }

    /// 批次點陣化到資料夾，檔名格式與 Python 版一致：`page_0001.png`（從 1 起算）。
    ///
    /// * `pages`：只處理指定頁（1 起算，與 Python 版 `pages` 參數一致）；`None` 表示全部。
    /// * `force_regenerate`：`false` 時跳過已存在的檔案（Python 版預設行為）。
    ///
    /// 回傳實際存在（含先前已產生）的 PNG 檔名，依頁序。
    pub fn render_to_dir(
        &self,
        out_dir: impl AsRef<Path>,
        dpi: f64,
        pages: Option<&[usize]>,
        force_regenerate: bool,
    ) -> Result<Vec<String>> {
        let out_dir = out_dir.as_ref();
        std::fs::create_dir_all(out_dir)?;

        let mut names = Vec::new();
        for page_num in 1..=self.page_count() {
            if let Some(selected) = pages {
                if !selected.contains(&page_num) {
                    continue;
                }
            }
            let name = format!("page_{page_num:04}.png");
            let path = out_dir.join(&name);
            names.push(name);

            if !force_regenerate && path.exists() {
                continue;
            }
            let png = self.render_png(page_num - 1, dpi)?;
            std::fs::write(&path, png)?;
        }
        Ok(names)
    }

    fn page(&self, index: usize) -> Result<&hayro::hayro_syntax::page::Page<'_>> {
        let count = self.page_count();
        self.pdf
            .pages()
            .get(index)
            .ok_or(Error::PageOutOfRange { index, count })
    }
}

/// 找出「乘完不會被截斷掉一個像素」的最小 f32 縮放值。
fn scale_for(base: f64, target: u32) -> Result<f32> {
    let base = base as f32;
    if base <= 0.0 {
        return Err(Error::Render("頁面基準尺寸為 0".into()));
    }
    let mut scale = (target as f32 / base).max(f32::MIN_POSITIVE);
    // 最多推幾步就好；正常情況 0 或 1 步。
    for _ in 0..8 {
        if (base * scale) as u32 >= target {
            return Ok(scale);
        }
        scale = scale.next_up();
    }
    Err(Error::Render(format!(
        "無法把 {base} pt 對齊到 {target} px"
    )))
}

/// 便利函式：把 PDF 的每一頁轉成 PNG，回傳路徑清單。
pub fn pdf_to_png(
    pdf_path: impl AsRef<Path>,
    out_dir: impl AsRef<Path>,
    dpi: f64,
) -> Result<Vec<PathBuf>> {
    let doc = PdfDoc::open(pdf_path)?;
    let out_dir = out_dir.as_ref();
    let names = doc.render_to_dir(out_dir, dpi, None, false)?;
    Ok(names.into_iter().map(|n| out_dir.join(n)).collect())
}
