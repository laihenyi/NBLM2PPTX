//! MobileSAM「切出所有物件」：在攝影或漸層背景上找出獨立物件。
//!
//! 模型是 Acly/MobileSAM 的 ONNX 匯出（MIT；原始 MobileSAM 為 Apache-2.0）：
//!
//! - encoder：輸入長邊縮到 1024 的 0–255 RGB（`[H, W, 3]` f32），模型內部正規化
//!   與補邊，輸出 `image_embeddings [1, 256, 64, 64]`。
//! - decoder（multi）：一次一個提示點，輸出 4 個候選；第 0 個是單點歧義輸出，
//!   只取 1–3。
//!
//! 流程是 SAM 的 automatic mask generation：均勻格點逐點解碼 → IoU 與穩定度
//! 過濾 → 去重 → 碎片合併。所有比較都在 decoder 的 256×256 低解析度 logits
//! 上做，最後只把留下來的物件雙線性取樣到原圖尺寸（與 decoder 的
//! `orig_im_size` 輸出等價，但不必為每個物件再跑一次推論）。
//!
//! SAM 只產生「候選」：文字擦除後的塗抹塊、極光帶都可能得到高分。呼叫端要
//! 傳入擦字區域（[`SamOptions`] 的 `exclude`）把塗抹塊濾掉。

use std::path::{Path, PathBuf};

use n2p_core::{BBox, RgbImage};
use ort::session::Session;
use ort::value::Tensor;

use crate::mask::Mask;

/// encoder 模型檔名。
pub const ENCODER_FILE: &str = "mobile_sam_image_encoder.onnx";
/// decoder 模型檔名。
pub const DECODER_FILE: &str = "sam_mask_decoder_multi.onnx";
/// 下載來源。
pub const ENCODER_URL: &str =
    "https://huggingface.co/Acly/MobileSAM/resolve/main/mobile_sam_image_encoder.onnx";
pub const DECODER_URL: &str =
    "https://huggingface.co/Acly/MobileSAM/resolve/main/sam_mask_decoder_multi.onnx";
/// 預期檔案大小（用來偵測下載不完整）。
pub const ENCODER_SIZE: u64 = 28_157_093;
pub const DECODER_SIZE: u64 = 16_496_559;

/// encoder 的長邊。
const INPUT_SIDE: f64 = 1024.0;
/// decoder 低解析度遮罩的邊長（對應補邊後的 1024×1024）。
const LOW_RES: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum SamError {
    #[error("找不到 SAM 模型：{0}（執行 `n2p models fetch` 下載）")]
    Missing(PathBuf),
    #[error("SAM 模型不完整：{path}（{actual} / {expected} 位元組）")]
    Truncated {
        path: PathBuf,
        actual: u64,
        expected: u64,
    },
    #[error("ONNX Runtime 錯誤：{0}")]
    Runtime(String),
    #[error("SAM 輸出格式不符：{0}")]
    Output(String),
}

impl From<ort::Error> for SamError {
    fn from(error: ort::Error) -> Self {
        SamError::Runtime(error.to_string())
    }
}

/// 模型所在目錄（`N2P_SAM_DIR` 可覆寫）。
pub fn default_model_dir() -> Option<PathBuf> {
    if let Ok(custom) = std::env::var("N2P_SAM_DIR") {
        if !custom.is_empty() {
            return Some(PathBuf::from(custom));
        }
    }
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Caches"))?
    } else if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(xdg)
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".cache")
    };
    Some(base.join("n2p"))
}

/// 偵測參數。
#[derive(Debug, Clone)]
pub struct SamOptions<'a> {
    /// 每邊的提示點數（`grid × grid` 個點）。
    pub grid: usize,
    /// decoder 自評 IoU 下限。
    pub min_iou: f32,
    /// 穩定度下限：`|logit > 1| / |logit > -1|`。
    pub min_stability: f32,
    /// 「弱候選」的 IoU／穩定度下限。弱候選只有在能把兩個以上已留的碎片合成
    /// 整體時才採用（CH1 第 4 頁的雪花整體 IoU 只有 0.71）。
    pub weak_min_iou: f32,
    pub weak_min_stability: f32,
    /// 面積佔頁面比例的上下限。
    pub min_area_fraction: f64,
    pub max_area_fraction: f64,
    /// 外接框寬或高超過頁面此比例就丟掉（面板、整片光帶）。
    pub max_extent_fraction: f64,
    /// 貼著頁面邊緣的候選（被截半的裝飾）是否丟掉。
    pub drop_border_touching: bool,
    /// 候選有此比例以上落在 `exclude` 內就丟掉。
    pub max_excluded_fraction: f64,
    /// 要排除的區域（原圖座標），通常是擦字區域。
    pub exclude: Option<&'a Mask>,
}

impl Default for SamOptions<'_> {
    fn default() -> Self {
        Self {
            grid: 16,
            min_iou: 0.88,
            min_stability: 0.92,
            weak_min_iou: 0.70,
            weak_min_stability: 0.70,
            min_area_fraction: 0.0005,
            max_area_fraction: 0.25,
            max_extent_fraction: 0.8,
            drop_border_touching: true,
            max_excluded_fraction: 0.5,
            exclude: None,
        }
    }
}

/// 一個偵測到的物件（原圖座標）。
#[derive(Debug, Clone)]
pub struct SamObject {
    pub bbox: BBox,
    pub mask: Mask,
    pub score: f32,
}

/// 已載入的 MobileSAM。
pub struct Sam {
    encoder: Session,
    decoder: Session,
}

impl std::fmt::Debug for Sam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sam").finish_non_exhaustive()
    }
}

fn check_model(path: &Path, expected: u64) -> Result<(), SamError> {
    if !path.exists() {
        return Err(SamError::Missing(path.to_path_buf()));
    }
    let actual = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if actual < expected {
        return Err(SamError::Truncated {
            path: path.to_path_buf(),
            actual,
            expected,
        });
    }
    Ok(())
}

impl Sam {
    /// 從目錄載入 encoder 與 decoder。
    pub fn load(dir: &Path) -> Result<Self, SamError> {
        let encoder_path = dir.join(ENCODER_FILE);
        let decoder_path = dir.join(DECODER_FILE);
        check_model(&encoder_path, ENCODER_SIZE)?;
        check_model(&decoder_path, DECODER_SIZE)?;
        let environment = crate::runtime::environment().map_err(SamError::Runtime)?;
        // 註：encoder 推論時暫存用量約 5.4 GB（只載入模型約 0.2 GB）。關閉 CPU arena、
        // 記憶體樣式預配置或降低圖最佳化等級都量不出差別，是這份匯出本身的特性。
        let encoder = Session::builder(environment)?.commit_from_file(&encoder_path)?;
        let decoder = Session::builder(environment)?.commit_from_file(&decoder_path)?;
        Ok(Self { encoder, decoder })
    }

    /// 用預設目錄載入。
    pub fn load_default() -> Result<Self, SamError> {
        let dir =
            default_model_dir().ok_or_else(|| SamError::Missing(PathBuf::from(ENCODER_FILE)))?;
        Self::load(&dir)
    }

    /// 找出影像中的獨立物件。
    pub fn segment(
        &mut self,
        image: &RgbImage,
        options: &SamOptions<'_>,
    ) -> Result<Vec<SamObject>, SamError> {
        let (width, height) = (image.width as usize, image.height as usize);
        if width == 0 || height == 0 {
            return Ok(Vec::new());
        }
        let scale = INPUT_SIDE / width.max(height) as f64;
        let (input_w, input_h) = (
            ((width as f64 * scale).round() as usize).max(1),
            ((height as f64 * scale).round() as usize).max(1),
        );

        // 1. encoder
        let pixels = resize_bilinear(image, input_w, input_h);
        let input = Tensor::from_array(([input_h, input_w, 3], pixels))?;
        let outputs = self.encoder.run(ort::inputs!["input_image" => input])?;
        let (shape, data) = outputs["image_embeddings"]
            .try_extract_tensor::<f32>()
            .map_err(|e| SamError::Output(e.to_string()))?;
        if shape.len() != 4 || shape[1] != 256 {
            return Err(SamError::Output(format!("image_embeddings {shape:?}")));
        }
        let embeddings = data.to_vec();
        drop(outputs);

        // 2. 格點逐點解碼。低解析度遮罩只有左上 crop_w × crop_h 對應到影像，其餘是補邊。
        let crop_w = input_w.div_ceil(4).min(LOW_RES);
        let crop_h = input_h.div_ceil(4).min(LOW_RES);
        let grid = options.grid.max(1);
        let mut candidates = Vec::new();
        for gy in 0..grid {
            for gx in 0..grid {
                let px = (gx as f32 + 0.5) / grid as f32 * input_w as f32;
                let py = (gy as f32 + 0.5) / grid as f32 * input_h as f32;
                let low_res = self.decode_point(&embeddings, px, py, input_w, input_h)?;
                for (k, iou) in low_res.iou.iter().enumerate().skip(1) {
                    if *iou < options.weak_min_iou {
                        continue;
                    }
                    let plane = &low_res.masks[k * LOW_RES * LOW_RES..(k + 1) * LOW_RES * LOW_RES];
                    if let Some(mut candidate) = Candidate::from_logits(plane, crop_w, crop_h, *iou)
                        && candidate.stability >= options.weak_min_stability
                    {
                        candidate.weak =
                            *iou < options.min_iou || candidate.stability < options.min_stability;
                        candidates.push(candidate);
                    }
                }
            }
        }

        // 3. 幾何與排除區過濾
        let page = (crop_w * crop_h) as f64;
        candidates.retain(|c| {
            let fraction = c.area as f64 / page;
            let (bw, bh) = (c.bbox.2 - c.bbox.0, c.bbox.3 - c.bbox.1);
            fraction >= options.min_area_fraction
                && fraction <= options.max_area_fraction
                && (bw as f64) <= crop_w as f64 * options.max_extent_fraction
                && (bh as f64) <= crop_h as f64 * options.max_extent_fraction
                && !(options.drop_border_touching
                    && (c.bbox.0 == 0 || c.bbox.1 == 0 || c.bbox.2 >= crop_w || c.bbox.3 >= crop_h))
        });
        if let Some(exclude) = options.exclude {
            candidates.retain(|c| {
                excluded_fraction(c, crop_w, exclude, scale) <= options.max_excluded_fraction
            });
        }

        // 4. 去重與碎片合併
        let kept = select(&candidates, crop_w);

        // 5. 放大到原圖
        Ok(kept
            .into_iter()
            .filter_map(|index| upsample(&candidates[index], crop_w, crop_h, image, scale))
            .collect())
    }

    fn decode_point(
        &mut self,
        embeddings: &[f32],
        x: f32,
        y: f32,
        input_w: usize,
        input_h: usize,
    ) -> Result<LowRes, SamError> {
        let inputs = ort::inputs![
            "image_embeddings" => Tensor::from_array(([1usize, 256, 64, 64], embeddings.to_vec()))?,
            // 單點提示要補一個 (0, 0)、label -1 的填充點（SAM ONNX 匯出的慣例）。
            "point_coords" => Tensor::from_array(([1usize, 2, 2], vec![x, y, 0.0, 0.0]))?,
            "point_labels" => Tensor::from_array(([1usize, 2], vec![1.0f32, -1.0]))?,
            "mask_input" => Tensor::from_array(([1usize, 1, 256, 256], vec![0.0f32; 256 * 256]))?,
            "has_mask_input" => Tensor::from_array(([1usize], vec![0.0f32]))?,
            // 只用低解析度輸出；高解析度輸出取最小以省時間。
            "orig_im_size" => Tensor::from_array(([2usize], vec![(input_h / 4).max(1) as f32, (input_w / 4).max(1) as f32]))?,
        ];
        let outputs = self.decoder.run(inputs)?;
        let (shape, masks) = outputs["low_res_masks"]
            .try_extract_tensor::<f32>()
            .map_err(|e| SamError::Output(e.to_string()))?;
        if shape.len() != 4 || shape[1] != 4 || shape[2] != LOW_RES as i64 {
            return Err(SamError::Output(format!("low_res_masks {shape:?}")));
        }
        let masks = masks.to_vec();
        let (_, iou) = outputs["iou_predictions"]
            .try_extract_tensor::<f32>()
            .map_err(|e| SamError::Output(e.to_string()))?;
        Ok(LowRes {
            masks,
            iou: iou.to_vec(),
        })
    }
}

static SHARED: std::sync::OnceLock<std::sync::Mutex<Option<Sam>>> = std::sync::OnceLock::new();

/// 整個行程共用的引擎（第一次呼叫時載入；載入失敗或釋放後為 `None`，由使用端重新載入）。
pub fn shared() -> &'static std::sync::Mutex<Option<Sam>> {
    SHARED.get_or_init(|| std::sync::Mutex::new(Sam::load_default().ok()))
}

/// 釋放共用引擎與它的推論暫存（encoder 約 5 GB）。長駐的行程（GUI）在工作之間呼叫。
pub fn release_shared() {
    if let Some(engine) = SHARED.get() {
        *engine.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

struct LowRes {
    masks: Vec<f32>,
    iou: Vec<f32>,
}

/// 低解析度（crop 範圍內）的一個候選遮罩。
#[derive(Debug, Clone)]
struct Candidate {
    /// crop 範圍內的 logits，`crop_w × crop_h`。
    logits: Vec<f32>,
    bits: Vec<bool>,
    area: usize,
    /// (x0, y0, x1, y1)，右下不含。
    bbox: (usize, usize, usize, usize),
    score: f32,
    stability: f32,
    /// 低信心候選：只能用來把碎片合成整體，不能單獨成為物件。
    weak: bool,
}

impl Candidate {
    fn from_logits(plane: &[f32], crop_w: usize, crop_h: usize, score: f32) -> Option<Self> {
        let mut logits = Vec::with_capacity(crop_w * crop_h);
        let (mut strict, mut loose) = (0usize, 0usize);
        for y in 0..crop_h {
            for x in 0..crop_w {
                let value = plane[y * LOW_RES + x];
                logits.push(value);
                strict += (value > 1.0) as usize;
                loose += (value > -1.0) as usize;
            }
        }
        let stability = if loose == 0 {
            0.0
        } else {
            strict as f32 / loose as f32
        };
        Self::from_parts(logits, crop_w, score, stability)
    }

    fn from_parts(mut logits: Vec<f32>, crop_w: usize, score: f32, stability: f32) -> Option<Self> {
        // SAM 的遮罩常夾帶不相連的小塊（相鄰鍋子的把手，CH3 第 5 頁）。只留最大的
        // 連通塊，被丟掉的像素把 logit 壓成負值，放大到原圖時才不會復活。
        let bits = largest_component(&logits, crop_w);
        for (value, bit) in logits.iter_mut().zip(&bits) {
            if !bit && *value > 0.0 {
                *value = -1.0;
            }
        }
        let mut bbox = (usize::MAX, usize::MAX, 0, 0);
        let mut area = 0;
        for (index, bit) in bits.iter().enumerate() {
            if *bit {
                let (x, y) = (index % crop_w, index / crop_w);
                bbox = (
                    bbox.0.min(x),
                    bbox.1.min(y),
                    bbox.2.max(x + 1),
                    bbox.3.max(y + 1),
                );
                area += 1;
            }
        }
        (area > 0).then_some(Self {
            logits,
            bits,
            area,
            bbox,
            score,
            stability,
            weak: false,
        })
    }
}

/// `logit > 0` 的最大 8 連通塊。
fn largest_component(logits: &[f32], width: usize) -> Vec<bool> {
    let height = logits.len() / width.max(1);
    let mut label = vec![0u32; logits.len()];
    let (mut best, mut best_size, mut next) = (0u32, 0usize, 0u32);
    let mut stack = Vec::new();
    for start in 0..logits.len() {
        if logits[start] <= 0.0 || label[start] != 0 {
            continue;
        }
        next += 1;
        label[start] = next;
        stack.push(start);
        let mut size = 0;
        while let Some(index) = stack.pop() {
            size += 1;
            let (x, y) = (index % width, index / width);
            for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    let neighbour = ny * width + nx;
                    if logits[neighbour] > 0.0 && label[neighbour] == 0 {
                        label[neighbour] = next;
                        stack.push(neighbour);
                    }
                }
            }
        }
        if size > best_size {
            best_size = size;
            best = next;
        }
    }
    label.iter().map(|l| *l != 0 && *l == best).collect()
}

/// `a` 有多少像素同時在 `b` 內。
fn intersection(a: &Candidate, b: &Candidate, crop_w: usize) -> usize {
    let (x0, y0) = (a.bbox.0.max(b.bbox.0), a.bbox.1.max(b.bbox.1));
    let (x1, y1) = (a.bbox.2.min(b.bbox.2), a.bbox.3.min(b.bbox.3));
    if x0 >= x1 || y0 >= y1 {
        return 0;
    }
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let index = y * crop_w + x;
            count += (a.bits[index] && b.bits[index]) as usize;
        }
    }
    count
}

/// 兩塊遮罩是否相接（間距不超過 1 個低解析度像素，約原圖 3–11 px）。
fn touching(a: &Candidate, b: &Candidate, crop_w: usize) -> bool {
    const GAP: usize = 1;
    let (x0, y0) = (
        a.bbox.0.max(b.bbox.0.saturating_sub(GAP)),
        a.bbox.1.max(b.bbox.1.saturating_sub(GAP)),
    );
    let (x1, y1) = (a.bbox.2.min(b.bbox.2 + GAP), a.bbox.3.min(b.bbox.3 + GAP));
    let crop_h = a.bits.len() / crop_w;
    for y in y0..y1 {
        for x in x0..x1 {
            if !a.bits[y * crop_w + x] {
                continue;
            }
            for ny in y.saturating_sub(GAP)..=(y + GAP).min(crop_h - 1) {
                for nx in x.saturating_sub(GAP)..=(x + GAP).min(crop_w - 1) {
                    if b.bits[ny * crop_w + nx] {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// 去重與碎片合併，回傳留下來的候選索引。
///
/// 1. 依分數由高到低，與已留者 IoU > 0.95 的視為重複。
/// 2. 依面積由小到大。新候選「包住」已留的碎片時：
///    - 碎片佔新候選不到一半（雪花之於冰晶碎片）→ 以整體取代碎片；
///    - 碎片幾乎組成整個新候選且彼此相接（鍋身與把手）→ 以整體取代碎片；
///    - 碎片之間有底色縫隙（兩個勾選圖示被併成一塊）→ 丟掉這個合併遮罩。
/// 3. 沒包住任何碎片、卻與已留者大量重疊的候選丟掉，避免同一塊像素屬於兩個物件。
fn select(candidates: &[Candidate], crop_w: usize) -> Vec<usize> {
    // 只處理幾乎相同的遮罩；「整體 vs. 碎片」（IoU 常在 0.85–0.9）留給第 2 步。
    const DUPLICATE_IOU: f64 = 0.95;
    const CONTAINED: f64 = 0.8;
    const MAX_PARTIAL_OVERLAP: f64 = 0.3;
    // 碎片解釋不到這個比例，大候選就是含有大量新像素的「整體」。
    const WHOLE_EXPLAINED: f64 = 0.5;

    let mut by_score: Vec<usize> = (0..candidates.len()).collect();
    by_score.sort_by(|a, b| candidates[*b].score.total_cmp(&candidates[*a].score));
    let mut unique: Vec<usize> = Vec::new();
    for index in by_score {
        let c = &candidates[index];
        let duplicate = unique.iter().any(|k| {
            let other = &candidates[*k];
            let inter = intersection(c, other, crop_w) as f64;
            inter / (c.area as f64 + other.area as f64 - inter) > DUPLICATE_IOU
        });
        if !duplicate {
            unique.push(index);
        }
    }

    unique.sort_by_key(|i| candidates[*i].area);
    let mut kept: Vec<usize> = Vec::new();
    for index in unique {
        let c = &candidates[index];
        let mut contained = Vec::new();
        let mut overlap = 0usize;
        for k in &kept {
            let piece = &candidates[*k];
            let inter = intersection(piece, c, crop_w);
            overlap += inter;
            if inter as f64 >= piece.area as f64 * CONTAINED {
                contained.push(*k);
            }
        }
        if c.weak {
            // 弱候選只用來合成整體：至少包住兩個碎片、且碎片只解釋不到一半。
            let explained: usize = contained
                .iter()
                .map(|k| intersection(&candidates[*k], c, crop_w))
                .sum();
            let others = overlap - explained;
            if contained.len() >= 2
                && (explained as f64) < c.area as f64 * WHOLE_EXPLAINED
                && (others as f64) <= c.area as f64 * MAX_PARTIAL_OVERLAP
            {
                kept.retain(|k| !contained.contains(k));
                kept.push(index);
            }
            continue;
        }
        if contained.is_empty() {
            if (overlap as f64) <= c.area as f64 * MAX_PARTIAL_OVERLAP {
                kept.push(index);
            }
            continue;
        }
        // 碎片只解釋了大候選的一小部分 → 大候選是整體（雪花之於冰晶碎片）。
        // 碎片幾乎就是大候選的全部 → 看碎片是否相接：相接是同一物件的零件，
        // 有縫隙則是 SAM 把兩個獨立物件併在一起。
        let explained: usize = contained
            .iter()
            .map(|k| intersection(&candidates[*k], c, crop_w))
            .sum();
        let is_whole = (explained as f64) < c.area as f64 * WHOLE_EXPLAINED
            || connected(&contained, candidates, crop_w);
        if is_whole {
            kept.retain(|k| !contained.contains(k));
            kept.push(index);
        }
    }
    kept
}

/// 這些碎片是否彼此相接成一群。
fn connected(pieces: &[usize], candidates: &[Candidate], crop_w: usize) -> bool {
    if pieces.len() <= 1 {
        return true;
    }
    let mut reached = vec![false; pieces.len()];
    reached[0] = true;
    let mut frontier = vec![0];
    while let Some(i) = frontier.pop() {
        for j in 0..pieces.len() {
            if !reached[j] && touching(&candidates[pieces[i]], &candidates[pieces[j]], crop_w) {
                reached[j] = true;
                frontier.push(j);
            }
        }
    }
    reached.iter().all(|r| *r)
}

/// 候選有多少比例落在排除區內（以每個低解析度像素中心對應的原圖像素判斷）。
fn excluded_fraction(c: &Candidate, crop_w: usize, exclude: &Mask, scale: f64) -> f64 {
    let mut hit = 0usize;
    for y in c.bbox.1..c.bbox.3 {
        for x in c.bbox.0..c.bbox.2 {
            if !c.bits[y * crop_w + x] {
                continue;
            }
            let ox = (((x as f64 + 0.5) * 4.0) / scale) as u32;
            let oy = (((y as f64 + 0.5) * 4.0) / scale) as u32;
            if ox < exclude.width && oy < exclude.height && exclude.get(ox, oy) {
                hit += 1;
            }
        }
    }
    hit as f64 / c.area as f64
}

/// 把低解析度 logits 雙線性取樣到原圖尺寸，以 0 為門檻。
fn upsample(
    c: &Candidate,
    crop_w: usize,
    crop_h: usize,
    image: &RgbImage,
    scale: f64,
) -> Option<SamObject> {
    let to_low = scale / 4.0;
    // 只算候選外接框（外擴 1 格）對應的原圖範圍。
    let left = ((c.bbox.0.saturating_sub(1)) as f64 / to_low)
        .floor()
        .max(0.0) as u32;
    let top = ((c.bbox.1.saturating_sub(1)) as f64 / to_low)
        .floor()
        .max(0.0) as u32;
    let right = (((c.bbox.2 + 1) as f64 / to_low).ceil() as u32).min(image.width);
    let bottom = (((c.bbox.3 + 1) as f64 / to_low).ceil() as u32).min(image.height);
    let sample = |u: f64, v: f64| -> f32 {
        let u = u.clamp(0.0, (crop_w - 1) as f64);
        let v = v.clamp(0.0, (crop_h - 1) as f64);
        let (x0, y0) = (u.floor() as usize, v.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(crop_w - 1), (y0 + 1).min(crop_h - 1));
        let (fx, fy) = ((u - x0 as f64) as f32, (v - y0 as f64) as f32);
        let at = |x: usize, y: usize| c.logits[y * crop_w + x];
        let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        top * (1.0 - fy) + bottom * fy
    };
    let mut mask = Mask::new(image.width, image.height);
    for y in top..bottom {
        let v = (y as f64 + 0.5) * to_low - 0.5;
        for x in left..right {
            let u = (x as f64 + 0.5) * to_low - 0.5;
            if sample(u, v) > 0.0 {
                mask.set(x, y, true);
            }
        }
    }
    let bbox = mask.bbox()?;
    Some(SamObject {
        bbox,
        mask,
        score: c.score,
    })
}

/// 雙線性縮放成 `[h, w, 3]` 的 f32（0–255）。
fn resize_bilinear(image: &RgbImage, width: usize, height: usize) -> Vec<f32> {
    let (sw, sh) = (image.width as usize, image.height as usize);
    let (rx, ry) = (sw as f64 / width as f64, sh as f64 / height as f64);
    let mut out = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        let fy = ((y as f64 + 0.5) * ry - 0.5).clamp(0.0, (sh - 1) as f64);
        let (y0, wy) = (fy.floor() as usize, (fy - fy.floor()) as f32);
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..width {
            let fx = ((x as f64 + 0.5) * rx - 0.5).clamp(0.0, (sw - 1) as f64);
            let (x0, wx) = (fx.floor() as usize, (fx - fx.floor()) as f32);
            let x1 = (x0 + 1).min(sw - 1);
            let p = |x: usize, y: usize| image.pixel(x as u32, y as u32);
            let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
            for channel in 0..3 {
                let top = a[channel] as f32 * (1.0 - wx) + b[channel] as f32 * wx;
                let bottom = c[channel] as f32 * (1.0 - wx) + d[channel] as f32 * wx;
                out.push(top * (1.0 - wy) + bottom * wy);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 40;
    const H: usize = 30;

    /// 以矩形清單建立候選（logits：內 +5、外 -5）。
    fn candidate(rects: &[(usize, usize, usize, usize)], score: f32) -> Candidate {
        let mut logits = vec![-5.0f32; W * H];
        for (x0, y0, x1, y1) in rects {
            for y in *y0..*y1 {
                for x in *x0..*x1 {
                    logits[y * W + x] = 5.0;
                }
            }
        }
        Candidate::from_parts(logits, W, score, 1.0).unwrap()
    }

    #[test]
    fn separated_icons_beat_their_merged_mask() {
        // 兩個勾選圖示之間隔著底色；合併遮罩應被丟掉。
        let a = candidate(&[(2, 2, 8, 8)], 0.95);
        let b = candidate(&[(2, 12, 8, 18)], 0.95);
        let merged = candidate(&[(2, 2, 8, 18)], 0.97);
        let kept = select(&[a, b, merged], W);
        let mut kept = kept;
        kept.sort();
        assert_eq!(kept, vec![0, 1]);
    }

    #[test]
    fn touching_fragments_are_replaced_by_the_whole() {
        // 鍋身與把手相接；整鍋遮罩應取代兩個碎片。
        let body = candidate(&[(10, 10, 20, 20)], 0.95);
        let handle = candidate(&[(20, 13, 24, 17)], 0.95);
        let whole = candidate(&[(10, 10, 20, 20), (20, 13, 24, 17)], 0.93);
        let kept = select(&[body, handle, whole], W);
        assert_eq!(kept, vec![2]);
    }

    #[test]
    fn whole_object_absorbs_scattered_fragments() {
        // 兩段冰晶碎片彼此不相接，但只佔雪花的一小部分 → 雪花取代碎片。
        let a = candidate(&[(10, 10, 12, 12)], 0.95);
        let b = candidate(&[(20, 20, 22, 22)], 0.95);
        let flake = candidate(&[(8, 8, 24, 24)], 0.93);
        assert_eq!(select(&[a, b, flake], W), vec![2]);
    }

    fn weak(rects: &[(usize, usize, usize, usize)]) -> Candidate {
        let mut c = candidate(rects, 0.7);
        c.weak = true;
        c
    }

    #[test]
    fn weak_candidate_is_used_only_to_join_fragments() {
        let a = candidate(&[(10, 10, 12, 12)], 0.95);
        let b = candidate(&[(20, 20, 22, 22)], 0.95);
        let flake = weak(&[(8, 8, 24, 24)]);
        let band = weak(&[(30, 0, 40, 30)]);
        let mut kept = select(&[a, b, flake, band], W);
        kept.sort();
        assert_eq!(kept, vec![2]);
    }

    #[test]
    fn stray_disconnected_blob_is_removed() {
        let c = candidate(&[(2, 2, 12, 12), (20, 2, 22, 4)], 0.9);
        assert_eq!(c.area, 100);
        assert_eq!(c.bbox, (2, 2, 12, 12));
    }

    #[test]
    fn near_duplicates_keep_the_higher_score() {
        let a = candidate(&[(5, 5, 15, 15)], 0.90);
        let b = candidate(&[(5, 5, 15, 15)], 0.96);
        assert_eq!(select(&[a, b], W), vec![1]);
    }

    #[test]
    fn partially_overlapping_candidate_is_dropped() {
        let a = candidate(&[(5, 5, 15, 15)], 0.95);
        let b = candidate(&[(10, 5, 20, 15)], 0.95);
        assert_eq!(select(&[a, b], W).len(), 1);
    }

    #[test]
    fn excluded_region_is_measured_in_image_coordinates() {
        // scale = 1：低解析度一格對應原圖 4 px。
        let c = candidate(&[(0, 0, 4, 4)], 0.9);
        let mut exclude = Mask::new(160, 120);
        for y in 0..8 {
            for x in 0..16 {
                exclude.set(x, y, true);
            }
        }
        // 左上 4×4 格中，上半兩列（原圖 y < 8）被排除。
        assert!((excluded_fraction(&c, W, &exclude, 1.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn missing_model_reports_path() {
        let dir = std::env::temp_dir().join("n2p-sam-missing");
        match Sam::load(&dir) {
            Err(SamError::Missing(path)) => assert!(path.ends_with(ENCODER_FILE)),
            other => panic!("預期 Missing，實際 {other:?}"),
        }
    }

    #[test]
    fn truncated_model_is_rejected() {
        let dir = std::env::temp_dir().join("n2p-sam-truncated");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(ENCODER_FILE), b"short").unwrap();
        assert!(matches!(Sam::load(&dir), Err(SamError::Truncated { .. })));
    }
}
