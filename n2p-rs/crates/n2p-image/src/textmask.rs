//! 文字區域分析：建出「只含文字筆畫」的遮罩，並取出文字色與字重。
//!
//! 方法參考 `Appt-OCR`（`appt_ocr/image.py:analyze_text_features`）：
//!
//! 1. **極性由框線亮度決定** —— 文字是「與邊框不同」的像素，邊框比 127 暗就是
//!    淺色字（暗底），否則深色字。這比「比對取樣文字色的亮度」穩，因為取樣
//!    文字色本身就容易被光暈帶偏。
//! 2. **全域 Otsu ∪ 局部自適應** —— 兩者互補：Otsu 抓對比強的字，自適應抓
//!    亮度不均（漸層、光束、木紋）背景上的字。
//! 3. **形態學閉運算** 補斷筆。
//! 4. 文字色取 Otsu 遮罩內像素的平均；字重由遮罩密度判斷。
//!
//! 這取代了先前那一層「顏色距離 / 亮度極性 / 彩度」的手工規則 —— 那些規則
//! 本質上都在猜同一件事，而 Otsu ∪ 自適應是這件事的標準解法。

use n2p_core::{BBox, RgbImage};

use crate::mask::Mask;

/// 邊框平均亮度低於此值視為暗底（淺色字）。
const LIGHT_BACKGROUND_THRESHOLD: f64 = 127.0;
/// 自適應門檻的常數偏移。
const ADAPTIVE_C: f64 = 5.0;
/// 自適應視窗最小邊長（須為奇數）。
const ADAPTIVE_MIN_BLOCK: usize = 11;
/// 閉運算核大小 = 單行字高 / 此值。
const CLOSE_KERNEL_DIVISOR: usize = 25;
/// 閉運算核大小的上限（像素）。核一旦大於半個筆畫間隙，就會把字糊成一塊。
const CLOSE_KERNEL_MAX: usize = 4;
/// 遮罩密度超過此值視為粗體。
const BOLD_DENSITY: f64 = 0.25;

/// 一個文字區域的分析結果。
#[derive(Debug, Clone)]
pub struct TextAnalysis {
    /// 只含文字筆畫的遮罩（整張影像座標）。
    pub mask: Mask,
    /// 文字主色（Otsu 遮罩內像素的平均）。
    pub color: [u8; 3],
    /// 是否為粗體（筆畫密度）。
    pub is_bold: bool,
    /// 筆畫佔區域的比例，供診斷使用。
    pub density: f64,
}

#[inline]
fn luma(rgb: [u8; 3]) -> u8 {
    ((0.2126 * rgb[0] as f64 + 0.7152 * rgb[1] as f64 + 0.0722 * rgb[2] as f64) + 0.5) as u8
}

/// 分析一個矩形區域內的文字：回傳遮罩、顏色、字重。
///
/// `line_height_px` 是**單行**字高（不是整個區塊的高度）。閉運算的核由此
/// 決定 —— 用區塊高度會讓多行標題的核大到把筆畫糊成一片（CH7 第 1 頁）。
pub fn analyze_with_line_height(image: &RgbImage, bbox: BBox, line_height_px: f64) -> TextAnalysis {
    let bbox = bbox.clamp(image.width as i64, image.height as i64);
    let w = bbox.width().max(1) as usize;
    let h = bbox.height().max(1) as usize;
    let mut mask = Mask::new(image.width, image.height);
    if bbox.is_empty() {
        return TextAnalysis {
            mask,
            color: [0, 0, 0],
            is_bold: false,
            density: 0.0,
        };
    }

    // 灰階
    let mut gray = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            gray[y * w + x] =
                luma(image.pixel((bbox.left + x as i64) as u32, (bbox.top + y as i64) as u32));
        }
    }

    // 極性：由框線（去掉四角，避免斜切）平均亮度決定
    let (mut border_sum, mut border_count) = (0u64, 0u64);
    for x in 0..w {
        for y in [0usize, h - 1] {
            border_sum += gray[y * w + x] as u64;
            border_count += 1;
        }
    }
    for y in 1..h.saturating_sub(1) {
        for x in [0usize, w - 1] {
            border_sum += gray[y * w + x] as u64;
            border_count += 1;
        }
    }
    let border_mean = if border_count > 0 {
        border_sum as f64 / border_count as f64
    } else {
        255.0
    };
    let light_text = border_mean < LIGHT_BACKGROUND_THRESHOLD; // 暗底 → 淺色字

    // 策略 1：全域 Otsu
    let otsu = otsu_threshold(&gray);
    let mut hit_otsu = vec![false; w * h];
    for (index, value) in gray.iter().enumerate() {
        hit_otsu[index] = if light_text {
            *value as f64 > otsu
        } else {
            (*value as f64) < otsu
        };
    }

    // 策略 2：局部自適應（用積分影像取局部平均，取代 OpenCV 的 Gaussian）
    let block = ADAPTIVE_MIN_BLOCK.max((w.min(h) / 4) | 1);
    let mut integral = vec![0u64; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row_sum = 0u64;
        for x in 0..w {
            row_sum += gray[y * w + x] as u64;
            integral[(y + 1) * (w + 1) + (x + 1)] = integral[y * (w + 1) + (x + 1)] + row_sum;
        }
    }
    let radius = (block / 2) as i64;
    let mut hit_adaptive = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let x0 = (x as i64 - radius).max(0) as usize;
            let y0 = (y as i64 - radius).max(0) as usize;
            let x1 = ((x as i64 + radius) as usize).min(w - 1);
            let y1 = ((y as i64 + radius) as usize).min(h - 1);
            let area = ((x1 - x0 + 1) * (y1 - y0 + 1)) as f64;
            let sum = integral[(y1 + 1) * (w + 1) + (x1 + 1)] + integral[y0 * (w + 1) + x0]
                - integral[y0 * (w + 1) + (x1 + 1)]
                - integral[(y1 + 1) * (w + 1) + x0];
            let local_mean = sum as f64 / area;
            let value = gray[y * w + x] as f64;
            hit_adaptive[y * w + x] = if light_text {
                value > local_mean + ADAPTIVE_C
            } else {
                value < local_mean - ADAPTIVE_C
            };
        }
    }

    // 聯集
    let mut combined: Vec<bool> = hit_otsu
        .iter()
        .zip(hit_adaptive.iter())
        .map(|(a, b)| *a || *b)
        .collect();

    // 形態學閉運算（先膨脹再侵蝕），核為近似圓形
    let kernel = ((line_height_px / CLOSE_KERNEL_DIVISOR as f64).round() as usize)
        .clamp(1, CLOSE_KERNEL_MAX);
    combined = dilate(&combined, w, h, kernel);
    combined = erode(&combined, w, h, kernel);

    // 文字投影／外框：原稿常有「白字 + 深色投影」的樣式，只擦白字會留下一圈
    // 髒邊（CH7 第 5、9 頁）。從遮罩往外長幾輪，只吃「比背景更暗（或更亮）
    // 且與筆畫相鄰」的像素 —— 與文字無關的深色圖案通常在更遠處。
    let grow_radius = ((line_height_px * 0.10).round() as usize).clamp(1, 8);
    let margin = 18.0;
    expand_into_contrast_neighbours(
        &mut combined,
        &gray,
        w,
        h,
        light_text,
        border_mean,
        grow_radius,
        margin,
    );

    // 文字色：取 Otsu 遮罩內像素的平均（比聯集乾淨）
    let mut sums = [0u64; 3];
    let mut count = 0u64;
    for y in 0..h {
        for x in 0..w {
            if hit_otsu[y * w + x] {
                let pixel =
                    image.pixel((bbox.left + x as i64) as u32, (bbox.top + y as i64) as u32);
                sums[0] += pixel[0] as u64;
                sums[1] += pixel[1] as u64;
                sums[2] += pixel[2] as u64;
                count += 1;
            }
        }
    }
    let color = match count {
        0 => [0, 0, 0],
        n => [
            (sums[0] / n) as u8,
            (sums[1] / n) as u8,
            (sums[2] / n) as u8,
        ],
    };

    let mut masked = 0usize;
    for y in 0..h {
        for x in 0..w {
            if combined[y * w + x] {
                mask.set(
                    (bbox.left + x as i64) as u32,
                    (bbox.top + y as i64) as u32,
                    true,
                );
                masked += 1;
            }
        }
    }
    let area = (w * h) as f64;
    TextAnalysis {
        mask,
        color,
        is_bold: count as f64 / area > BOLD_DENSITY,
        density: masked as f64 / area,
    }
}

/// 把遮罩往「與背景有明顯亮度差」的相鄰像素擴張（吃投影、描邊與抗鋸齒）。
///
/// `rounds` 是擴張輪數（每輪 1 像素），只有亮度**絕對差**超過 `margin` 的像素
/// 會被吃進來，所以不會一路長到背景紋理上。
///
/// 方向必須雙向：CH7 是「白字 + 深色投影」（要往暗長），CH9 是「深藍字 +
/// 白色描邊」（要往亮長）。只往文字極性那一側長，另一種樣式就會留下髒邊。
#[allow(clippy::too_many_arguments)]
fn expand_into_contrast_neighbours(
    mask: &mut [bool],
    gray: &[u8],
    w: usize,
    h: usize,
    _light_text: bool,
    border_mean: f64,
    rounds: usize,
    margin: f64,
) {
    for _ in 0..rounds {
        let mut additions = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let index = y * w + x;
                if mask[index] {
                    continue;
                }
                let value = gray[index] as f64;
                if (value - border_mean).abs() <= margin {
                    continue;
                }
                let neighbours = [
                    (x.wrapping_sub(1), y),
                    (x + 1, y),
                    (x, y.wrapping_sub(1)),
                    (x, y + 1),
                ];
                if neighbours
                    .iter()
                    .any(|(nx, ny)| *nx < w && *ny < h && mask[*ny * w + *nx])
                {
                    additions.push(index);
                }
            }
        }
        if additions.is_empty() {
            break;
        }
        for index in additions {
            mask[index] = true;
        }
    }
}

/// 以區域高度當單行字高來分析（單行區塊適用）。
pub fn analyze(image: &RgbImage, bbox: BBox) -> TextAnalysis {
    let line_height = bbox.height().max(1) as f64;
    analyze_with_line_height(image, bbox, line_height)
}

/// 量測區域外圍一圈的亮度中位數絕對偏差，用來判斷背景「平坦」還是「有紋理」。
///
/// 平坦背景（色塊、漸層）→ 可以把文字光暈整片擦掉再補回來；
/// 有紋理的背景（木紋、照片）→ 擦掉大面積只會補成一塊模糊色塊，
/// 這時寧可留著光暈（它剛好變成新文字的外光）。
pub fn ring_luma_mad(image: &RgbImage, bbox: BBox, band: i64) -> f64 {
    let band = band.max(2);
    let outer = BBox::new(
        bbox.left - band,
        bbox.top - band,
        bbox.right + band,
        bbox.bottom + band,
    )
    .clamp(image.width as i64, image.height as i64);
    if outer.is_empty() {
        return 0.0;
    }
    let mut samples = Vec::new();
    for y in outer.top..outer.bottom {
        for x in outer.left..outer.right {
            let inside = x >= bbox.left && x < bbox.right && y >= bbox.top && y < bbox.bottom;
            if inside {
                continue;
            }
            samples.push(luma(image.pixel(x as u32, y as u32)) as f64);
        }
    }
    if samples.is_empty() {
        return 0.0;
    }
    // 用中位數絕對偏差而不是標準差：環上常夾著裝飾（引號、標靶圖案），
    // 標準差會被那幾個極端值撐大，把「平坦藍底」誤判成「有紋理」。
    let center = median(&mut samples);
    let mut deviations: Vec<f64> = samples.iter().map(|v| (v - center).abs()).collect();
    median(&mut deviations)
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

/// 以 Otsu 法求全域門檻。
fn otsu_threshold(gray: &[u8]) -> f64 {
    let mut histogram = [0u64; 256];
    for value in gray {
        histogram[*value as usize] += 1;
    }
    let total: u64 = gray.len() as u64;
    if total == 0 {
        return 127.0;
    }
    let sum_all: f64 = histogram
        .iter()
        .enumerate()
        .map(|(value, count)| value as f64 * *count as f64)
        .sum();

    let (mut sum_background, mut weight_background) = (0.0f64, 0u64);
    let (mut best_variance, mut best_threshold) = (-1.0f64, 127.0f64);
    for (value, count) in histogram.iter().enumerate() {
        weight_background += count;
        if weight_background == 0 {
            continue;
        }
        let weight_foreground = total - weight_background;
        if weight_foreground == 0 {
            break;
        }
        sum_background += value as f64 * *count as f64;
        let mean_background = sum_background / weight_background as f64;
        let mean_foreground = (sum_all - sum_background) / weight_foreground as f64;
        let variance = weight_background as f64
            * weight_foreground as f64
            * (mean_background - mean_foreground).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best_threshold = value as f64;
        }
    }
    best_threshold
}

fn dilate(source: &[bool], w: usize, h: usize, radius: usize) -> Vec<bool> {
    let mut out = vec![false; source.len()];
    for y in 0..h {
        for x in 0..w {
            if !source[y * w + x] {
                continue;
            }
            let y0 = y.saturating_sub(radius);
            let y1 = (y + radius).min(h - 1);
            let x0 = x.saturating_sub(radius);
            let x1 = (x + radius).min(w - 1);
            for ny in y0..=y1 {
                for nx in x0..=x1 {
                    out[ny * w + nx] = true;
                }
            }
        }
    }
    out
}

fn erode(source: &[bool], w: usize, h: usize, radius: usize) -> Vec<bool> {
    let mut out = vec![false; source.len()];
    for y in 0..h {
        for x in 0..w {
            if !source[y * w + x] {
                continue;
            }
            let y0 = y.saturating_sub(radius);
            let y1 = (y + radius).min(h - 1);
            let x0 = x.saturating_sub(radius);
            let x1 = (x + radius).min(w - 1);
            let mut all = true;
            'outer: for ny in y0..=y1 {
                for nx in x0..=x1 {
                    if !source[ny * w + nx] {
                        all = false;
                        break 'outer;
                    }
                }
            }
            out[y * w + x] = all;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stripes_image(bg: [u8; 3], fg: [u8; 3]) -> RgbImage {
        let mut image = RgbImage::filled(200, 60, bg);
        for y in 10..50 {
            for x in 10..190 {
                if (x - 10) % 24 < 4 {
                    image.set_pixel(x, y, fg);
                }
            }
        }
        image
    }

    #[test]
    fn dark_text_on_light_background() {
        let image = stripes_image([250, 250, 250], [10, 10, 10]);
        let analysis = analyze(&image, BBox::new(5, 5, 195, 55));
        assert!(
            analysis.density > 0.05,
            "應偵測到筆畫，實際 {}",
            analysis.density
        );
        assert!(
            analysis.color[0] < 60,
            "文字色應為深色，實際 {:?}",
            analysis.color
        );
        // 筆畫位置被遮罩、空白處沒有
        assert!(analysis.mask.get(10, 30));
        assert!(!analysis.mask.get(20, 30));
    }

    #[test]
    fn light_text_on_dark_background() {
        // 這是 CH7 的情況：暗底白字。舊的「顏色距離」在這裡會失效。
        let image = stripes_image([25, 25, 30], [250, 250, 250]);
        let analysis = analyze(&image, BBox::new(5, 5, 195, 55));
        assert!(analysis.density > 0.05);
        assert!(
            analysis.color[0] > 180,
            "文字色應為淺色，實際 {:?}",
            analysis.color
        );
        assert!(analysis.mask.get(10, 30));
        assert!(!analysis.mask.get(20, 30));
    }

    #[test]
    fn low_contrast_text_is_still_detected() {
        // CH7 第 3 頁的失敗案例：白字壓在淺色木地板上，對比很低。
        // 自適應門檻應該還是抓得到。
        let image = stripes_image([150, 145, 140], [200, 195, 190]);
        let analysis = analyze(&image, BBox::new(5, 5, 195, 55));
        assert!(
            analysis.density > 0.03,
            "低對比也要抓到，實際 {}",
            analysis.density
        );
    }

    #[test]
    fn uniform_region_yields_tiny_mask() {
        let image = RgbImage::filled(100, 40, [128, 128, 128]);
        let analysis = analyze(&image, BBox::new(10, 5, 90, 35));
        assert!(
            analysis.density < 0.02,
            "純色區域不該有筆畫，實際 {}",
            analysis.density
        );
    }

    #[test]
    fn otsu_finds_reasonable_threshold_for_two_peaks() {
        let mut gray = vec![10u8; 500];
        gray.extend(std::iter::repeat_n(240u8, 500));
        let threshold = otsu_threshold(&gray);
        assert!((10.0..=240.0).contains(&threshold), "實際 {threshold}");
    }

    #[test]
    fn bold_detection_follows_density() {
        let thin = stripes_image([250, 250, 250], [0, 0, 0]);
        // 把條紋加粗
        let mut thick = RgbImage::filled(200, 60, [250, 250, 250]);
        for y in 10..50 {
            for x in 10..190 {
                if (x - 10) % 16 < 10 {
                    thick.set_pixel(x, y, [0, 0, 0]);
                }
            }
        }
        let thin_analysis = analyze(&thin, BBox::new(5, 5, 195, 55));
        let thick_analysis = analyze(&thick, BBox::new(5, 5, 195, 55));
        assert!(
            thick_analysis.density > thin_analysis.density,
            "粗筆畫密度應更高：{} vs {}",
            thick_analysis.density,
            thin_analysis.density
        );
    }
}
