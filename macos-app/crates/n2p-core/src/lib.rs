//! 共用型別：緊密排列的 8-bit RGB 點陣圖與整數邊界框。
//!
//! 刻意不用 `image` crate 的 `RgbImage`，因為本專案的像素運算需要
//! 「直通 RGB、無 alpha、可廉價切片」這三個性質，而且錯誤型別要能自訂。

use std::path::Path;

/// 緊密排列的 8-bit RGB 點陣圖。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbImage {
    pub width: u32,
    pub height: u32,
    /// `width * height * 3` bytes，逐列排列。
    pub data: Vec<u8>,
}

/// 錯誤型別。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O 失敗：{0}")]
    Io(#[from] std::io::Error),

    #[error("PNG 解碼失敗：{0}")]
    Decode(String),

    #[error("PNG 編碼失敗：{0}")]
    Encode(String),

    #[error("點陣圖長度不符：預期 {expected} bytes，實際 {actual}")]
    LengthMismatch { expected: usize, actual: usize },

    #[error("尺寸不合法：{0}")]
    BadDimensions(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl RgbImage {
    /// 建立全黑影像。
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; (width as usize) * (height as usize) * 3],
        }
    }

    /// 建立單色影像。
    pub fn filled(width: u32, height: u32, rgb: [u8; 3]) -> Self {
        let mut data = vec![0u8; (width as usize) * (height as usize) * 3];
        for chunk in data.chunks_exact_mut(3) {
            chunk.copy_from_slice(&rgb);
        }
        Self {
            width,
            height,
            data,
        }
    }

    /// 由既有緩衝建立，會檢查長度。
    pub fn from_raw(width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        let expected = (width as usize) * (height as usize) * 3;
        if data.len() != expected {
            return Err(Error::LengthMismatch {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    pub fn pixel_count(&self) -> usize {
        (self.width as usize) * (self.height as usize)
    }

    /// 某一列的 RGB bytes。
    pub fn row(&self, y: u32) -> &[u8] {
        let stride = (self.width as usize) * 3;
        let start = (y as usize) * stride;
        &self.data[start..start + stride]
    }

    pub fn row_mut(&mut self, y: u32) -> &mut [u8] {
        let stride = (self.width as usize) * 3;
        let start = (y as usize) * stride;
        &mut self.data[start..start + stride]
    }

    /// 讀取單一像素。
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let index = ((y as usize) * (self.width as usize) + (x as usize)) * 3;
        [self.data[index], self.data[index + 1], self.data[index + 2]]
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, rgb: [u8; 3]) {
        let index = ((y as usize) * (self.width as usize) + (x as usize)) * 3;
        self.data[index..index + 3].copy_from_slice(&rgb);
    }

    /// 以 `bbox` 裁切。
    pub fn crop(&self, bbox: BBox) -> RgbImage {
        let bbox = bbox.clamp(self.width as i64, self.height as i64);
        let width = (bbox.right - bbox.left).max(0) as u32;
        let height = (bbox.bottom - bbox.top).max(0) as u32;
        let mut out = RgbImage::new(width, height);
        for y in 0..height {
            let src = self.row(bbox.top as u32 + y);
            let src_start = (bbox.left as usize) * 3;
            let dst = out.row_mut(y);
            dst.copy_from_slice(&src[src_start..src_start + (width as usize) * 3]);
        }
        out
    }

    /// 解碼 PNG。
    pub fn from_png_bytes(bytes: &[u8]) -> Result<Self> {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder
            .read_info()
            .map_err(|e| Error::Decode(e.to_string()))?;
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| Error::Decode("PNG 太大，無法配置輸出緩衝".into()))?;
        let mut buf = vec![0u8; size];
        let info = reader
            .next_frame(&mut buf)
            .map_err(|e| Error::Decode(e.to_string()))?;
        let (width, height, channels) = (info.width, info.height, info.color_type);
        buf.truncate(info.buffer_size());

        let rgb = match channels {
            png::ColorType::Rgb => buf,
            png::ColorType::Rgba => {
                let mut out = Vec::with_capacity((width as usize) * (height as usize) * 3);
                for pixel in buf.chunks_exact(4) {
                    out.extend_from_slice(&pixel[..3]);
                }
                out
            }
            png::ColorType::Grayscale => {
                let mut out = Vec::with_capacity((width as usize) * (height as usize) * 3);
                for value in &buf {
                    out.extend_from_slice(&[*value, *value, *value]);
                }
                out
            }
            other => return Err(Error::Decode(format!("不支援的 PNG 色彩型別：{other:?}"))),
        };
        Self::from_raw(width, height, rgb)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_png_bytes(&std::fs::read(path)?)
    }

    /// 編碼成 PNG bytes。
    pub fn to_png(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| Error::Encode(e.to_string()))?;
            writer
                .write_image_data(&self.data)
                .map_err(|e| Error::Encode(e.to_string()))?;
        }
        Ok(out)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        std::fs::write(path, self.to_png()?)?;
        Ok(())
    }
}

/// 整數像素邊界框，語意為 `[left, right) x [top, bottom)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BBox {
    pub left: i64,
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
}

impl BBox {
    pub fn new(left: i64, top: i64, right: i64, bottom: i64) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// 由四邊浮點座標四捨五入而來（對應 numpy 版的 `round()`）。
    pub fn from_f64(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        Self {
            left: left.round() as i64,
            top: top.round() as i64,
            right: right.round() as i64,
            bottom: bottom.round() as i64,
        }
    }

    pub fn width(&self) -> i64 {
        self.right - self.left
    }

    pub fn height(&self) -> i64 {
        self.bottom - self.top
    }

    pub fn is_empty(&self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }

    /// 夾到 `[0, w) x [0, h)` 之內。
    pub fn clamp(&self, width: i64, height: i64) -> Self {
        Self {
            left: self.left.clamp(0, width),
            top: self.top.clamp(0, height),
            right: self.right.clamp(0, width),
            bottom: self.bottom.clamp(0, height),
        }
    }

    /// 依比例縮放（對應 python 版的 `math.floor/ceil` 邊界處理交給呼叫端）。
    pub fn scale(&self, factor: f64) -> Self {
        Self::from_f64(
            self.left as f64 * factor,
            self.top as f64 * factor,
            self.right as f64 * factor,
            self.bottom as f64 * factor,
        )
    }

    /// 交併比（IOU），用於 MinerU 區塊比對。
    pub fn iou(&self, other: &BBox) -> f64 {
        let x1 = self.left.max(other.left);
        let y1 = self.top.max(other.top);
        let x2 = self.right.min(other.right);
        let y2 = self.bottom.min(other.bottom);
        let inter = (x2 - x1).max(0) as f64 * (y2 - y1).max(0) as f64;
        let area_a = self.width().max(0) as f64 * self.height().max(0) as f64;
        let area_b = other.width().max(0) as f64 * other.height().max(0) as f64;
        let union = area_a + area_b - inter;
        if union <= 0.0 { 0.0 } else { inter / union }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_roundtrip() {
        let img = RgbImage::filled(3, 2, [10, 20, 30]);
        assert_eq!(img.data.len(), 18);
        assert_eq!(img.pixel(2, 1), [10, 20, 30]);
        assert!(RgbImage::from_raw(3, 2, vec![0; 17]).is_err());
    }

    #[test]
    fn crop_extracts_window() {
        let mut img = RgbImage::new(4, 3);
        img.set_pixel(1, 1, [9, 9, 9]);
        let cropped = img.crop(BBox::new(1, 1, 3, 3));
        assert_eq!((cropped.width, cropped.height), (2, 2));
        assert_eq!(cropped.pixel(0, 0), [9, 9, 9]);
        assert_eq!(cropped.pixel(1, 1), [0, 0, 0]);
    }

    #[test]
    fn bbox_clamp_and_iou() {
        let b = BBox::new(-5, -5, 100, 100).clamp(50, 40);
        assert_eq!((b.left, b.top, b.right, b.bottom), (0, 0, 50, 40));

        let a = BBox::new(0, 0, 10, 10);
        assert_eq!(a.iou(&a), 1.0);
        assert_eq!(a.iou(&BBox::new(20, 20, 30, 30)), 0.0);
        // 半重疊：交 50，聯 150
        let close = (a.iou(&BBox::new(5, 0, 15, 10)) - 50.0 / 150.0).abs();
        assert!(close < 1e-12);
    }

    #[test]
    fn png_roundtrip_preserves_pixels() {
        let mut img = RgbImage::new(2, 2);
        img.set_pixel(0, 0, [1, 2, 3]);
        img.set_pixel(1, 1, [250, 251, 252]);
        let bytes = img.to_png().unwrap();
        let back = RgbImage::from_png_bytes(&bytes).unwrap();
        assert_eq!(back, img);
    }
}
