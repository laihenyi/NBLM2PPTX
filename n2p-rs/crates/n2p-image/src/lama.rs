//! LaMa（Resolution-Robust Large Mask Inpainting）推論引擎。
//!
//! 用途：把文字筆畫（以及緊貼筆畫的光暈）從背景抹掉，並由模型**重建**底下的
//! 紋理。我們自己的插值修補在平坦背景上夠用，但遇到木紋、照片、漸層光束時
//! 只會留下一塊模糊 —— 那正是 CH7 那類簡報的問題。
//!
//! ## 模型
//!
//! `lama_fp32.onnx`（Apache-2.0，約 199 MiB）。介面實測：
//!
//! - 輸入 `image`：`[1, 3, 512, 512] f32`，值域 0–1
//! - 輸入 `mask`：`[1, 1, 512, 512] f32`，0 = 保留、1 = 修補
//! - 輸出 `output`：`[1, 3, 512, 512] f32`，值域 0–255
//! - 空間維度**固定 512**，所以大區域必須切成 512 見方的圖塊
//!
//! 推論完全在本機，不連網；模型只在第一次使用前抓一次。
//!
//! ## 裝置
//!
//! macOS 上預設用 GPU（CoreML）：第一次載入時把模型改寫成 CoreML 能整張接手的版本
//! （見 [`crate::gpu`]），存在原模型旁。GPU 不可用時退回純 CPU。
//! `N2P_LAMA_DEVICE=gpu|cpu|auto` 可強制指定（預設 `auto`）。

use std::path::{Path, PathBuf};

use n2p_core::{BBox, RgbImage};
use ort::ep::coreml::{ComputeUnits, ModelFormat};
use ort::session::Session;
use ort::session::builder::{AutoDevicePolicy, SessionBuilder};
use ort::value::Tensor;

use crate::mask::Mask;

/// 模型要求的圖塊邊長。
pub const TILE: usize = 512;
/// 相鄰圖塊的重疊像素。
const OVERLAP: usize = 128;
/// 只在遮罩外圍多帶這麼多背景，讓模型有足夠線索。
const CONTEXT_PAD: i64 = 48;
/// 混合權重的羽化寬度（圖塊邊緣附近權重趨近 0，避免接縫）。
const FEATHER: f32 = 32.0;

/// 模型檔名。
pub const MODEL_FILE: &str = "lama_fp32.onnx";
/// 下載來源。
pub const MODEL_URL: &str = "https://huggingface.co/Carve/LaMa-ONNX/resolve/main/lama_fp32.onnx";
/// 模型大小（位元組），用來偵測下載未完成。
pub const MODEL_SIZE: u64 = 208_044_816;

#[derive(Debug, thiserror::Error)]
pub enum LamaError {
    #[error(
        "找不到 LaMa 模型：{0}（可執行 `n2p models fetch` 下載，或用 N2P_LAMA_MODEL 指定路徑）"
    )]
    Missing(PathBuf),
    #[error("模型檔案不完整：{path}（{actual} / {expected} 位元組），請重新下載")]
    Truncated {
        path: PathBuf,
        actual: u64,
        expected: u64,
    },
    #[error("ONNX Runtime 錯誤：{0}")]
    Runtime(String),
    #[error("推論輸出格式不符預期：{0}")]
    Output(String),
}

impl From<ort::Error> for LamaError {
    fn from(error: ort::Error) -> Self {
        LamaError::Runtime(error.to_string())
    }
}

/// 模型預設快取位置。
pub fn default_model_path() -> Option<PathBuf> {
    if let Ok(custom) = std::env::var("N2P_LAMA_MODEL") {
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
    Some(base.join("n2p").join(MODEL_FILE))
}

/// LaMa 實際推論的裝置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// CoreML（Metal GPU）。
    Gpu,
    Cpu,
}

/// 使用者要求的裝置（`N2P_LAMA_DEVICE`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceRequest {
    /// 能用 GPU 就用，否則 CPU。
    Auto,
    Gpu,
    Cpu,
}

impl DeviceRequest {
    pub fn from_env() -> Self {
        match std::env::var("N2P_LAMA_DEVICE").as_deref() {
            Ok("gpu") => Self::Gpu,
            Ok("cpu") => Self::Cpu,
            _ => Self::Auto,
        }
    }
}

/// 已載入的 LaMa 引擎。
pub struct Lama {
    session: Session,
    device: Device,
}

impl std::fmt::Debug for Lama {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lama")
            .field("device", &self.device)
            .finish_non_exhaustive()
    }
}

/// 不讓 ort 自動掛執行提供者：它在 macOS 預設會掛上 NeuralNetwork 格式的 CoreML，
/// 把圖切成 CPU／CoreML 交錯的碎片——實測比純 CPU 慢約 2.5 倍、載入多 11 秒，
/// 精度也只剩 fp16。
fn session_builder() -> Result<SessionBuilder, LamaError> {
    let environment = crate::runtime::environment().map_err(LamaError::Runtime)?;
    Session::builder(environment)?
        .with_auto_device(AutoDevicePolicy::PreferCPU)
        .map_err(|e| LamaError::Runtime(e.to_string()))
}

/// 在 GPU 上載入轉換後的模型（必要時先轉換）。
fn load_gpu(original: &Path) -> Result<Session, LamaError> {
    if !cfg!(target_os = "macos") {
        return Err(LamaError::Runtime("GPU 推論只支援 macOS".into()));
    }
    let converted = crate::gpu::ensure_converted(original).map_err(LamaError::Runtime)?;
    let cache = crate::gpu::compile_cache_dir(original);
    let mut coreml = ort::ep::CoreML::default()
        .with_model_format(ModelFormat::MLProgram)
        .with_static_input_shapes(true)
        // 實測 Neural Engine（fp32 模型）比 GPU 慢 5 倍
        .with_compute_units(ComputeUnits::CPUAndGPU);
    if std::fs::create_dir_all(&cache).is_ok() {
        coreml = coreml.with_model_cache_dir(cache.to_string_lossy());
    }
    let session = session_builder()?
        .with_execution_providers([coreml.build().error_on_failure()])
        .map_err(|e| LamaError::Runtime(e.to_string()))?
        .commit_from_file(&converted)?;
    Ok(session)
}

impl Lama {
    /// 從指定路徑載入模型，裝置依 `N2P_LAMA_DEVICE`（預設能用 GPU 就用）。
    pub fn load(path: &Path) -> Result<Self, LamaError> {
        Self::load_on(path, DeviceRequest::from_env())
    }

    /// 從指定路徑載入模型到指定裝置。`Auto` 在 GPU 失敗時退回 CPU。
    pub fn load_on(path: &Path, request: DeviceRequest) -> Result<Self, LamaError> {
        if !path.exists() {
            return Err(LamaError::Missing(path.to_path_buf()));
        }
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if size < MODEL_SIZE {
            return Err(LamaError::Truncated {
                path: path.to_path_buf(),
                actual: size,
                expected: MODEL_SIZE,
            });
        }
        // 非 macOS 的 `Auto` 直接用 CPU，不必每次印一行退回警告
        let try_gpu = match request {
            DeviceRequest::Cpu => false,
            DeviceRequest::Gpu => true,
            DeviceRequest::Auto => cfg!(target_os = "macos"),
        };
        if try_gpu {
            match load_gpu(path) {
                Ok(session) => {
                    return Ok(Self {
                        session,
                        device: Device::Gpu,
                    });
                }
                Err(error) if request == DeviceRequest::Gpu => return Err(error),
                Err(error) => eprintln!("  ! LaMa 無法使用 GPU（{error}），改用 CPU"),
            }
        }
        let session = session_builder()?.commit_from_file(path)?;
        Ok(Self {
            session,
            device: Device::Cpu,
        })
    }

    /// 實際推論的裝置。
    pub fn device(&self) -> Device {
        self.device
    }

    /// 用預設快取路徑載入。
    pub fn load_default() -> Result<Self, LamaError> {
        let path =
            default_model_path().ok_or_else(|| LamaError::Missing(PathBuf::from(MODEL_FILE)))?;
        Self::load(&path)
    }

    /// 若模型可用就載入，否則回傳 `None`（呼叫端可退回插值修補）。
    pub fn load_if_available() -> Option<Self> {
        Self::load_default().ok()
    }

    /// 就地修補遮罩標記的像素。
    ///
    /// 只覆寫遮罩內的像素；遮罩外一個像素都不會變動。
    pub fn inpaint_mask(&mut self, image: &mut RgbImage, mask: &Mask) -> Result<(), LamaError> {
        let Some(mask_bbox) = mask.bbox() else {
            return Ok(());
        };
        let region = BBox::new(
            mask_bbox.left - CONTEXT_PAD,
            mask_bbox.top - CONTEXT_PAD,
            mask_bbox.right + CONTEXT_PAD,
            mask_bbox.bottom + CONTEXT_PAD,
        )
        .clamp(image.width as i64, image.height as i64);
        if region.is_empty() {
            return Ok(());
        }

        // 以「加權累加」混合重疊圖塊：每個像素的權重是它離圖塊邊界的距離，
        // 換句話說越靠中央的預測越可信。
        let region_width = region.width().max(1) as usize;
        let region_height = region.height().max(1) as usize;
        let mut accumulated = vec![0.0f32; region_width * region_height * 3];
        let mut weights = vec![0.0f32; region_width * region_height];
        // 從未修改的影像取圖塊線索：否則前一塊的填補結果會被後一塊當成背景，
        // 錯誤逐塊累積。
        let pristine = image.clone();

        for tile_top in tile_positions(region.top, region.bottom) {
            for tile_left in tile_positions(region.left, region.right) {
                let tile = BBox::new(
                    tile_left,
                    tile_top,
                    tile_left + TILE as i64,
                    tile_top + TILE as i64,
                );
                let Some(prediction) = self.run_tile(&pristine, mask, tile)? else {
                    continue;
                };
                for y in 0..TILE {
                    for x in 0..TILE {
                        let (px, py) = (tile_left + x as i64, tile_top + y as i64);
                        if px < region.left
                            || px >= region.right
                            || py < region.top
                            || py >= region.bottom
                        {
                            continue;
                        }
                        if !mask.get(px as u32, py as u32) {
                            continue;
                        }
                        let weight = feather_weight(x, y);
                        if weight <= 0.0 {
                            continue;
                        }
                        let index =
                            (py - region.top) as usize * region_width + (px - region.left) as usize;
                        weights[index] += weight;
                        for channel in 0..3 {
                            accumulated[index * 3 + channel] +=
                                prediction[(y * TILE + x) * 3 + channel] * weight;
                        }
                    }
                }
            }
        }

        for y in 0..region_height {
            for x in 0..region_width {
                let index = y * region_width + x;
                let weight = weights[index];
                if weight <= 0.0 {
                    continue;
                }
                let px = (region.left + x as i64) as u32;
                let py = (region.top + y as i64) as u32;
                let pixel = [
                    (accumulated[index * 3] / weight).round().clamp(0.0, 255.0) as u8,
                    (accumulated[index * 3 + 1] / weight)
                        .round()
                        .clamp(0.0, 255.0) as u8,
                    (accumulated[index * 3 + 2] / weight)
                        .round()
                        .clamp(0.0, 255.0) as u8,
                ];
                image.set_pixel(px, py, pixel);
            }
        }
        Ok(())
    }

    /// 跑一個 512×512 圖塊；整塊都不需要修補時回傳 `None`。
    ///
    /// 圖塊超出影像邊界時以邊緣像素重複填充（`replicate`），避免在邊界產生
    /// 假的黑色漸變。
    fn run_tile(
        &mut self,
        image: &RgbImage,
        mask: &Mask,
        tile: BBox,
    ) -> Result<Option<Vec<f32>>, LamaError> {
        let mut needs = false;
        for y in 0..TILE {
            for x in 0..TILE {
                if mask.get(
                    (tile.left + x as i64).clamp(0, image.width as i64 - 1) as u32,
                    (tile.top + y as i64).clamp(0, image.height as i64 - 1) as u32,
                ) {
                    needs = true;
                    break;
                }
            }
            if needs {
                break;
            }
        }
        if !needs {
            return Ok(None);
        }

        let mut pixels = vec![0.0f32; 3 * TILE * TILE];
        let mut mask_values = vec![0.0f32; TILE * TILE];
        for y in 0..TILE {
            for x in 0..TILE {
                let source_x = (tile.left + x as i64).clamp(0, image.width as i64 - 1) as u32;
                let source_y = (tile.top + y as i64).clamp(0, image.height as i64 - 1) as u32;
                let pixel = image.pixel(source_x, source_y);
                for channel in 0..3 {
                    pixels[channel * TILE * TILE + y * TILE + x] = pixel[channel] as f32 / 255.0;
                }
                if mask.get(
                    (tile.left + x as i64).clamp(0, image.width as i64 - 1) as u32,
                    (tile.top + y as i64).clamp(0, image.height as i64 - 1) as u32,
                ) {
                    mask_values[y * TILE + x] = 1.0;
                }
            }
        }

        let image_tensor = Tensor::from_array(([1usize, 3, TILE, TILE], pixels))?;
        let mask_tensor = Tensor::from_array(([1usize, 1, TILE, TILE], mask_values))?;
        let outputs = self.session.run(ort::inputs![image_tensor, mask_tensor])?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| LamaError::Output(e.to_string()))?;
        if shape.len() != 4 || shape[1] != 3 {
            return Err(LamaError::Output(format!("{:?}", shape)));
        }
        let mut arranged = vec![0.0f32; TILE * TILE * 3];
        let plane = TILE * TILE;
        for y in 0..TILE {
            for x in 0..TILE {
                for channel in 0..3 {
                    arranged[(y * TILE + x) * 3 + channel] = data[channel * plane + y * TILE + x];
                }
            }
        }
        Ok(Some(arranged))
    }
}

/// 整個行程共用的引擎。
///
/// 每頁各建一次 session 會出問題：208 MiB 的模型在一個行程裡反覆載入，
/// 實測 11 頁的簡報會有 2 頁建立失敗（CH3 實測），而且每頁都要多花幾秒。
static SHARED: std::sync::OnceLock<std::sync::Mutex<Option<Lama>>> = std::sync::OnceLock::new();

/// 取得共用引擎（第一次呼叫時載入；載入失敗或釋放後為 `None`，由使用端重新載入）。
pub fn shared() -> &'static std::sync::Mutex<Option<Lama>> {
    SHARED.get_or_init(|| std::sync::Mutex::new(Lama::load_default().ok()))
}

/// 釋放共用引擎與它的推論暫存（數 GB）。長駐的行程（GUI）在工作之間呼叫。
pub fn release_shared() {
    if let Some(engine) = SHARED.get() {
        *engine.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// 圖塊左上角的起點序列，確保覆蓋 `[from, to)`。
fn tile_positions(from: i64, to: i64) -> Vec<i64> {
    let length = (to - from).max(1);
    if length <= TILE as i64 {
        // 單一圖塊也要往外挪，否則區域邊界正好落在圖塊邊緣（權重 0）。
        let shift = ((TILE as i64 - length) / 2).min(OVERLAP as i64);
        return vec![from - shift];
    }
    let step = (TILE - OVERLAP) as i64;
    let mut positions = Vec::new();
    let mut start = from;
    loop {
        if start + TILE as i64 >= to {
            positions.push(to - TILE as i64);
            break;
        }
        positions.push(start);
        start += step;
    }
    positions
}

/// 圖塊內的羽化權重：離邊界越遠權重越高。
///
/// 下限不是 0 —— 只被單一圖塊覆蓋的遮罩像素（區域邊界）若權重為 0，
/// 就會完全沒被填入，原稿文字留成鬼影（CH7 第 9 頁實測）。
fn feather_weight(x: usize, y: usize) -> f32 {
    let to_edge = x.min(y).min(TILE - 1 - x).min(TILE - 1 - y) as f32;
    (to_edge / FEATHER).clamp(0.02, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_positions_cover_small_regions_once() {
        // 單一圖塊會往外挪，讓區域邊界落在圖塊內部（否則邊界權重為 0）
        let positions = tile_positions(1000, 1200);
        assert_eq!(positions.len(), 1);
        assert!(positions[0] <= 1000 && positions[0] + TILE as i64 >= 1200);
    }

    #[test]
    fn tile_positions_cover_large_regions_completely() {
        let positions = tile_positions(0, 1300);
        assert_eq!(*positions.last().unwrap(), 1300 - TILE as i64);
        for window in positions.windows(2) {
            assert!(window[1] - window[0] <= (TILE - OVERLAP) as i64, "不能有縫");
        }
        // 起點到終點之間必須被完整覆蓋
        let mut covered = 0i64;
        for start in &positions {
            covered = covered.max(start + TILE as i64);
        }
        assert!(covered >= 1300);
    }

    #[test]
    fn feather_is_small_at_edges_and_one_in_the_middle() {
        // 邊緣不能是 0：只被單一圖塊覆蓋的像素會被完全跳過，留下原稿鬼影
        assert!(feather_weight(0, 100) > 0.0);
        assert!(feather_weight(0, 100) < feather_weight(16, 100));
        assert_eq!(feather_weight(100, 100), 1.0);
    }

    /// 真實模型存在時才跑：GPU 與 CPU 修補同一張圖，結果幾乎相同。
    #[test]
    fn gpu_and_cpu_agree_on_the_real_model() {
        let Some(path) = default_model_path().filter(|p| p.exists()) else {
            eprintln!("略過：沒有 LaMa 模型");
            return;
        };
        if !cfg!(target_os = "macos") {
            return;
        }
        let mut image = RgbImage::new(600, 400);
        for y in 0..400 {
            for x in 0..600 {
                image.set_pixel(
                    x,
                    y,
                    [(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8],
                );
            }
        }
        let mut mask = Mask::new(600, 400);
        for y in 170..230 {
            for x in 200..400 {
                mask.set(x, y, true);
            }
        }
        let mut results = Vec::new();
        for request in [DeviceRequest::Gpu, DeviceRequest::Cpu] {
            let mut lama = Lama::load_on(&path, request).expect("載入失敗");
            let expected = if request == DeviceRequest::Gpu {
                Device::Gpu
            } else {
                Device::Cpu
            };
            assert_eq!(lama.device(), expected);
            let mut copy = image.clone();
            lama.inpaint_mask(&mut copy, &mask).unwrap();
            results.push(copy);
        }
        let mut worst = 0;
        for y in 0..400 {
            for x in 0..600 {
                let (a, b) = (results[0].pixel(x, y), results[1].pixel(x, y));
                for c in 0..3 {
                    worst = worst.max(a[c].abs_diff(b[c]));
                }
            }
        }
        assert!(worst <= 1, "GPU 與 CPU 最大差 {worst}");
        let converted = crate::gpu::converted_path(&path);
        let ops = crate::gpu::op_histogram_of(&converted).unwrap();
        for gone in ["Einsum", "Neg", "Gather"] {
            assert!(!ops.contains_key(gone), "轉換檔仍有 {gone}");
        }
    }

    #[test]
    fn missing_model_reports_path() {
        let path = PathBuf::from("/nonexistent/lama_fp32.onnx");
        match Lama::load(&path) {
            Err(LamaError::Missing(reported)) => assert_eq!(reported, path),
            other => panic!("應該回報找不到模型，實際 {:?}", other.is_ok()),
        }
    }

    #[test]
    fn truncated_model_is_rejected() {
        let dir = std::env::temp_dir().join("n2p-lama-truncated-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("partial.onnx");
        std::fs::write(&path, b"not a model").unwrap();
        let result = Lama::load(&path);
        assert!(
            matches!(result, Err(LamaError::Truncated { .. })),
            "應拒絕不完整模型"
        );
        std::fs::remove_file(&path).ok();
    }
}
