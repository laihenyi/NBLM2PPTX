//! 開發用的環境變數：預載檔案、自動開始、自動取消、定時截圖（只截本視窗）。
//! 讓不能點滑鼠的自動化檢查也能走完整個介面流程。一般使用時全部不設。
//! App Store 版（feature `appstore`）啟動時會清掉所有 `N2P_*` 變數，這些掛鉤因此不會生效。
//!
//! | 變數 | 作用 |
//! | :--- | :--- |
//! | `N2P_GUI_FILES` | 預載的 PDF，以換行分隔 |
//! | `N2P_GUI_OUT_DIR` | 輸出資料夾 |
//! | `N2P_GUI_MODE` | `standard`、`objects` 或 `both` |
//! | `N2P_GUI_PAGES` | 頁碼範圍 |
//! | `N2P_GUI_OVERWRITE` | 設了就勾選覆寫 |
//! | `N2P_GUI_ADVANCED` | 設了就展開「進階設定」（截圖檢查模型狀態用） |
//! | `N2P_GUI_AUTOFETCH` | 設了就自動下載缺少的模型（在自動開始之前） |
//! | `N2P_GUI_AUTOSTART` | 設了就自動開始，全部結束後再截一張並關閉視窗 |
//! | `N2P_GUI_CANCEL_AFTER` | 開始後幾秒按取消 |
//! | `N2P_GUI_SCREENSHOT_DIR` | 每隔 `N2P_GUI_SCREENSHOT_EVERY` 秒（預設 5）截一張 PNG |
//! | `N2P_GUI_WINDOW` | 視窗大小，例如 `960x600`（App Store 截圖用 16:10） |
//! | `N2P_GUI_LANG` | 介面語言（`zh-TW`、`zh-CN`、`en`、`ja`、`es`、`fr`） |

use std::path::PathBuf;
use std::time::{Duration, Instant};

/// `N2P_GUI_WINDOW=寬x高`（邏輯像素）。格式不對就當作沒設。
pub fn window_size() -> Option<[f32; 2]> {
    let value = std::env::var("N2P_GUI_WINDOW").ok()?;
    let (width, height) = value.split_once('x')?;
    Some([width.trim().parse().ok()?, height.trim().parse().ok()?])
}

use eframe::egui;

use crate::i18n::Language;
use crate::model::{OutputMode, Settings};

/// 預載到介面的設定。
#[derive(Default)]
pub struct Preset {
    pub pdfs: Vec<PathBuf>,
    pub mode: Option<OutputMode>,
    pub out_dir: Option<PathBuf>,
    pub pages: Option<String>,
    pub overwrite: bool,
    pub advanced_open: bool,
    pub lang: Option<Language>,
}

#[derive(Default)]
pub struct DebugHooks {
    preset: Option<Preset>,
    autofetch: bool,
    fetched: bool,
    autostart: bool,
    started: bool,
    started_at: Option<Instant>,
    cancel_after: Option<Duration>,
    cancelled: bool,
    screenshot_dir: Option<PathBuf>,
    screenshot_every: Duration,
    last_request: Option<Instant>,
    pending: bool,
    shots: u32,
    saw_running: bool,
    final_requested: bool,
}

impl DebugHooks {
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        let preset = Preset {
            pdfs: var("N2P_GUI_FILES")
                .map(|v| v.lines().map(PathBuf::from).collect())
                .unwrap_or_default(),
            mode: var("N2P_GUI_MODE").and_then(|v| match v.as_str() {
                "standard" => Some(OutputMode::Standard),
                "objects" => Some(OutputMode::Objects),
                "both" => Some(OutputMode::Both),
                _ => None,
            }),
            out_dir: var("N2P_GUI_OUT_DIR").map(PathBuf::from),
            pages: var("N2P_GUI_PAGES"),
            overwrite: var("N2P_GUI_OVERWRITE").is_some(),
            advanced_open: var("N2P_GUI_ADVANCED").is_some(),
            lang: var("N2P_GUI_LANG").and_then(|v| Language::from_tag(&v)),
        };
        Self {
            preset: Some(preset),
            autofetch: var("N2P_GUI_AUTOFETCH").is_some(),
            autostart: var("N2P_GUI_AUTOSTART").is_some(),
            cancel_after: var("N2P_GUI_CANCEL_AFTER")
                .and_then(|v| v.parse().ok())
                .map(Duration::from_secs_f64),
            screenshot_dir: var("N2P_GUI_SCREENSHOT_DIR").map(PathBuf::from),
            screenshot_every: Duration::from_secs_f64(
                var("N2P_GUI_SCREENSHOT_EVERY")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(5.0),
            ),
            ..Default::default()
        }
    }

    /// 取出預載設定（只會給一次）。
    pub fn take_preset(&mut self) -> Preset {
        self.preset.take().unwrap_or_default()
    }

    pub fn apply_settings(preset: &Preset, settings: &mut Settings) {
        if let Some(pages) = &preset.pages {
            settings.pages = pages.clone();
        }
    }

    /// 該不該自動下載模型（只觸發一次）。
    pub fn autofetch_due(&mut self) -> bool {
        let due = self.autofetch && !self.fetched;
        self.fetched = true;
        due
    }

    /// 該不該自動開始（只觸發一次）。
    pub fn autostart_due(&mut self) -> bool {
        if self.autostart && !self.started {
            self.started = true;
            self.started_at = Some(Instant::now());
            return true;
        }
        false
    }

    /// 該不該自動取消（只觸發一次）。
    pub fn cancel_due(&mut self, running: bool) -> bool {
        match (self.cancel_after, self.started_at) {
            (Some(after), Some(at)) if running && !self.cancelled && at.elapsed() >= after => {
                self.cancelled = true;
                true
            }
            _ => false,
        }
    }

    /// 每一幀呼叫：處理截圖，自動模式下工作全部結束就截最後一張並關閉視窗。
    pub fn tick(&mut self, ctx: &egui::Context, running: bool) {
        self.saw_running |= running;
        let Some(dir) = self.screenshot_dir.clone() else {
            if self.autostart && self.saw_running && !running {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        };

        // 收截圖
        let images: Vec<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
                .collect()
        });
        for image in images {
            self.pending = false;
            self.shots += 1;
            let _ = std::fs::create_dir_all(&dir);
            save_png(&image, &dir.join(format!("shot_{:03}.png", self.shots)));
            if self.final_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        let finished = self.autostart && self.saw_running && !running;
        let due = self
            .last_request
            .is_none_or(|at| at.elapsed() >= self.screenshot_every);
        if !self.pending && !self.final_requested && (due || finished) {
            // 先讓畫面穩定一幀再截
            if self.last_request.is_some() || ctx.cumulative_pass_nr() > 3 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.pending = true;
                self.last_request = Some(Instant::now());
                self.final_requested = finished;
            }
        }
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

fn save_png(image: &egui::ColorImage, path: &std::path::Path) {
    let [width, height] = image.size;
    let mut out = n2p_core::RgbImage::filled(width as u32, height as u32, [0, 0, 0]);
    for y in 0..height {
        for x in 0..width {
            let pixel = image.pixels[y * width + x];
            out.set_pixel(x as u32, y as u32, [pixel.r(), pixel.g(), pixel.b()]);
        }
    }
    let _ = out.save(path);
}
