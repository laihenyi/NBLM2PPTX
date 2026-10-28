//! 主視窗。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, RichText};
use n2p_image::models::{ModelFile, ModelState, Purpose};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSArray, NSFileManager, NSString, NSURL};

use crate::debug::DebugHooks;
use crate::model::{self, Job, OutputMode, Plan, Settings};
use crate::worker::{self, Event, Outcome};

const WARN: Color32 = Color32::from_rgb(0xC0, 0x7A, 0x00);
const ERROR: Color32 = Color32::from_rgb(0xC0, 0x30, 0x30);
const OK: Color32 = Color32::from_rgb(0x2E, 0x8B, 0x57);

/// 佇列中一個工作的狀態。
enum Status {
    Waiting,
    Running { fraction: f32, stage: String },
    Done { text_boxes: usize, pictures: usize },
    Skipped(Plan),
    Failed(String),
    Cancelled,
}

struct Row {
    job: Job,
    status: Status,
}

pub struct App {
    pdfs: Vec<PathBuf>,
    mode: OutputMode,
    out_dir: Option<PathBuf>,
    overwrite: bool,
    advanced_open: bool,
    settings: Settings,
    models: Vec<ModelFile>,
    models_error: Option<String>,
    fetching: bool,
    rows: Vec<Row>,
    running: bool,
    cancel: Arc<AtomicBool>,
    events_tx: Sender<Event>,
    events_rx: Receiver<Event>,
    message: Option<(Color32, String)>,
    debug: DebugHooks,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, debug: DebugHooks) -> Self {
        install_cjk_font(&cc.egui_ctx);
        let (events_tx, events_rx) = channel();
        let mut app = Self {
            pdfs: Vec::new(),
            mode: OutputMode::Standard,
            // 一般版預設與 PDF 同資料夾。App Store 版沒有預設：沙盒只開放使用者在面板選的資料夾
            out_dir: None,
            overwrite: false,
            advanced_open: false,
            settings: Settings::default(),
            models: Vec::new(),
            models_error: None,
            fetching: false,
            rows: Vec::new(),
            running: false,
            cancel: Arc::new(AtomicBool::new(false)),
            events_tx,
            events_rx,
            message: None,
            debug,
        };
        app.refresh_models();
        let preset = app.debug.take_preset();
        DebugHooks::apply_settings(&preset, &mut app.settings);
        model::add_pdfs(&mut app.pdfs, preset.pdfs);
        if let Some(mode) = preset.mode {
            app.mode = mode;
        }
        if let Some(dir) = preset.out_dir {
            app.out_dir = Some(dir);
        }
        app.overwrite = preset.overwrite;
        app.advanced_open = preset.advanced_open;
        app
    }

    fn refresh_models(&mut self) {
        match n2p_image::models::manifest() {
            Ok(models) => {
                self.models = models;
                self.models_error = None;
            }
            Err(error) => self.models_error = Some(error),
        }
    }

    fn ready(&self, purpose: Purpose) -> bool {
        !self.models.is_empty() && n2p_image::models::ready(&self.models, purpose)
    }

    fn start(&mut self, ctx: &egui::Context) {
        self.message = None;
        if cfg!(feature = "appstore") && self.out_dir.is_none() {
            self.out_dir = pick_output_dir();
            if self.out_dir.is_none() {
                self.message = Some((ERROR, "請先選擇輸出資料夾。".into()));
                return;
            }
        }
        let lama_ready = self.ready(Purpose::Erase);
        let planned = model::plan_jobs(
            &self.pdfs,
            self.mode,
            self.out_dir.as_deref(),
            self.overwrite,
        );
        let mut queue = Vec::new();
        self.rows.clear();
        for (index, (job, plan)) in planned.into_iter().enumerate() {
            let status = if plan == Plan::Convert {
                match model::convert_options(job.variant, &self.settings, lama_ready) {
                    Ok(options) => {
                        queue.push((index, job.clone(), options));
                        Status::Waiting
                    }
                    Err(error) => {
                        self.message = Some((ERROR, error));
                        self.rows.clear();
                        return;
                    }
                }
            } else {
                Status::Skipped(plan)
            };
            self.rows.push(Row { job, status });
        }
        if queue.is_empty() {
            self.message = Some((WARN, "沒有需要轉換的檔案。".into()));
            return;
        }
        self.cancel = Arc::new(AtomicBool::new(false));
        self.running = true;
        let ctx = ctx.clone();
        worker::spawn(
            queue,
            self.cancel.clone(),
            self.events_tx.clone(),
            move || ctx.request_repaint(),
        );
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                Event::Started { index } => {
                    self.rows[index].status = Status::Running {
                        fraction: 0.0,
                        stage: "準備中".into(),
                    }
                }
                Event::Progress { index, progress } => {
                    let (fraction, stage) = model::progress_fraction(progress);
                    self.rows[index].status = Status::Running { fraction, stage };
                }
                Event::Finished { index, result } => {
                    self.rows[index].status = match result {
                        Outcome::Done(report) => Status::Done {
                            text_boxes: report.text_boxes,
                            pictures: report.pictures,
                        },
                        Outcome::Failed(error) => Status::Failed(error),
                        Outcome::Cancelled => Status::Cancelled,
                    }
                }
                Event::Idle => {
                    self.running = false;
                    let done = self
                        .rows
                        .iter()
                        .filter(|r| matches!(r.status, Status::Done { .. }))
                        .count();
                    let failed = self
                        .rows
                        .iter()
                        .filter(|r| matches!(r.status, Status::Failed(_)))
                        .count();
                    self.message = Some(if failed > 0 {
                        (ERROR, format!("完成 {done} 份，失敗 {failed} 份。"))
                    } else if self.cancel.load(Ordering::Relaxed) {
                        (WARN, format!("已取消。完成 {done} 份。"))
                    } else {
                        (OK, format!("全部完成，共 {done} 份。"))
                    });
                }
                Event::Fetched(result) => {
                    self.fetching = false;
                    self.refresh_models();
                    self.message = Some(match result {
                        Ok(()) => (OK, "模型已下載完成。".into()),
                        Err(error) => (ERROR, format!("模型下載失敗：{error}")),
                    });
                }
            }
        }
    }

    fn overall_progress(&self) -> Option<(f32, String)> {
        let active: Vec<&Row> = self
            .rows
            .iter()
            .filter(|r| !matches!(r.status, Status::Skipped(_)))
            .collect();
        if active.is_empty() {
            return None;
        }
        let mut sum = 0.0;
        let mut label = String::new();
        for (position, row) in active.iter().enumerate() {
            sum += match &row.status {
                Status::Waiting => 0.0,
                Status::Running { fraction, stage } => {
                    label = format!(
                        "第 {}/{} 份：{}（{}）— {stage}",
                        position + 1,
                        active.len(),
                        file_name(&row.job.pdf),
                        row.job.variant.label()
                    );
                    *fraction
                }
                _ => 1.0,
            };
        }
        Some((sum / active.len() as f32, label))
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        // 拖放 PDF
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() && !self.running {
            let count = dropped.len();
            let added = model::add_pdfs(&mut self.pdfs, dropped);
            if added < count {
                self.message = Some((WARN, "只接受尚未加入的 PDF 檔。".into()));
            }
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());

        egui::TopBottomPanel::bottom("actions").show(ctx, |ui| {
            ui.add_space(8.0);
            self.actions(ui, ctx);
            ui.add_space(8.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("NotebookLM PDF → 可編輯 PowerPoint");
                ui.add_space(6.0);
                self.first_run(ui);
                self.sources(ui, hovering);
                ui.add_space(10.0);
                self.output_options(ui);
                ui.add_space(10.0);
                self.advanced(ui);
                ui.add_space(10.0);
                self.queue(ui);
            });
        });

        self.debug.tick(ctx, self.running);
        if self.debug.autofetch_due() && self.missing_models() {
            self.fetch(ctx);
        }
        if !self.fetching && self.debug.autostart_due() {
            self.start(ctx);
        }
        if self.debug.cancel_due(self.running) {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl App {
    fn missing_models(&self) -> bool {
        self.models.iter().any(|m| m.state() != ModelState::Ready)
    }

    fn fetch(&mut self, ctx: &egui::Context) {
        self.fetching = true;
        self.message = None;
        let ctx = ctx.clone();
        worker::fetch_models(self.events_tx.clone(), move || ctx.request_repaint());
    }

    /// 缺模型時放在最上方：第一次使用的人一定看得到。
    fn first_run(&mut self, ui: &mut egui::Ui) {
        if !self.fetching && !self.missing_models() {
            return;
        }
        let (done, total) = n2p_image::models::download_progress(&self.models);
        let mb = |bytes: u64| bytes as f64 / 1_000_000.0;
        section(ui, "首次使用：下載模型", |ui| {
            if self.fetching {
                ui.add(
                    egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(format!(
                        "下載中… {:.0} / {:.0} MB",
                        mb(done),
                        mb(total)
                    )),
                );
                // curl 在背景寫檔，定時重畫以更新進度
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(500));
            } else {
                ui.label(format!(
                    "去字與物件切出需要 AI 模型，只需下載一次（還差約 {:.0} MB）。",
                    mb(total - done)
                ));
                if ui
                    .add_enabled(!self.running, egui::Button::new("下載模型"))
                    .clicked()
                {
                    self.fetch(ui.ctx());
                }
                note(
                    ui,
                    "不下載也能轉換：標準版只擦文字筆畫，品質較差；物件版只處理平坦底色的頁面。",
                );
            }
        });
        ui.add_space(10.0);
    }

    fn sources(&mut self, ui: &mut egui::Ui, hovering: bool) {
        section(ui, "1. 來源 PDF", |ui| {
            if self.pdfs.is_empty() {
                let text = if hovering {
                    "放開以加入"
                } else {
                    "把 PDF 拖進視窗，或按「加入 PDF…」"
                };
                ui.label(RichText::new(text).weak());
            }
            let mut remove = None;
            for (index, pdf) in self.pdfs.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.running, egui::Button::new("移除").small())
                        .clicked()
                    {
                        remove = Some(index);
                    }
                    ui.label(file_name(pdf))
                        .on_hover_text(pdf.display().to_string());
                });
            }
            if let Some(index) = remove {
                self.pdfs.remove(index);
            }
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!self.running, |ui| {
                    if ui.button("加入 PDF…").clicked() {
                        if let Some(files) = rfd::FileDialog::new()
                            .add_filter("PDF", &["pdf", "PDF"])
                            .pick_files()
                        {
                            model::add_pdfs(&mut self.pdfs, files);
                        }
                    }
                    if !self.pdfs.is_empty() && ui.button("全部清除").clicked() {
                        self.pdfs.clear();
                        self.rows.clear();
                    }
                });
            });
        });
    }

    fn output_options(&mut self, ui: &mut egui::Ui) {
        let sam_ready = self.ready(Purpose::Objects);
        section(ui, "2. 輸出版本", |ui| {
            ui.add_enabled_ui(!self.running, |ui| {
                ui.radio_value(&mut self.mode, OutputMode::Standard, "標準版（建議）");
                note(
                    ui,
                    "文字變成可編輯的文字框，圖片留在背景。檔名結尾「_可編輯」。",
                );
                ui.radio_value(&mut self.mode, OutputMode::Objects, "物件版（實驗性）");
                note(
                    ui,
                    "另把圖示、插圖切成可移動的圖片。移走物件後，原位是補出來的背景，可能有暗塊。\
                     檔名結尾「_可編輯_物件」。",
                );
                ui.radio_value(&mut self.mode, OutputMode::Both, "兩種都產生");
            });
            if self.mode.needs_sam() && !sam_ready {
                ui.label(
                    RichText::new(
                        "物件版需要 SAM 模型。目前缺少，只有平坦底色的頁面能切出物件。請先下載模型。",
                    )
                    .color(WARN),
                );
            }
        });

        section(ui, "3. 輸出位置", |ui| {
            ui.add_enabled_ui(!self.running, |ui| {
                if cfg!(feature = "appstore") {
                    self.sandboxed_output_dir(ui);
                } else {
                    self.output_dir(ui);
                }
                ui.checkbox(&mut self.overwrite, "覆寫已存在的檔案");
                note(ui, "PowerPoint 正開著的檔案一律不覆寫。");
            });
        });
    }

    /// App Store 版：沙盒只能寫入使用者在面板裡選的資料夾。
    fn sandboxed_output_dir(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("存到");
            let (shown, full, button) = match &self.out_dir {
                Some(dir) => (display_name(dir), dir.display().to_string(), "更改…"),
                None => (
                    "（尚未選擇，開始轉換時會詢問）".to_owned(),
                    String::new(),
                    "選擇…",
                ),
            };
            let label = ui.add(egui::Label::new(shown).truncate());
            if !full.is_empty() {
                label.on_hover_text(full);
            }
            if ui.button(button).clicked() {
                if let Some(dir) = pick_output_dir() {
                    self.out_dir = Some(dir);
                }
            }
        });
    }

    /// 一般版：預設與 PDF 同資料夾，也可指定資料夾。
    fn output_dir(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut custom = self.out_dir.is_some();
            ui.radio_value(&mut custom, false, "與 PDF 相同的資料夾");
            ui.radio_value(&mut custom, true, "指定資料夾");
            if custom && self.out_dir.is_none() {
                self.out_dir = rfd::FileDialog::new().pick_folder();
            } else if !custom {
                self.out_dir = None;
            }
        });
        if let Some(shown) = self.out_dir.as_ref().map(|d| d.display().to_string()) {
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(shown).monospace()).truncate());
                if ui.button("更改…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.out_dir = Some(dir);
                    }
                }
            });
        }
    }

    fn advanced(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("進階設定")
            .default_open(self.advanced_open)
            .show(ui, |ui| {
                ui.add_enabled_ui(!self.running, |ui| {
                    ui.checkbox(
                        &mut self.settings.high_res_background,
                        "高解析度背景（較清晰，檔案較大）",
                    );
                    ui.checkbox(&mut self.settings.widescreen, "把頁面補成 16:9");
                    ui.horizontal(|ui| {
                        ui.label("頁碼範圍");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings.pages)
                                .hint_text("全部（例：1-3,5）")
                                .desired_width(160.0),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.label(RichText::new("模型").strong());
                if let Some(error) = &self.models_error {
                    ui.label(RichText::new(error).color(ERROR));
                }
                for model in &self.models {
                    let (color, text) = match model.state() {
                        ModelState::Ready => (OK, "已就緒".to_owned()),
                        ModelState::Missing => (WARN, "未下載".to_owned()),
                        ModelState::Partial { size } => (
                            WARN,
                            format!("不完整（{size} / {} 位元組）", model.expected_size),
                        ),
                    };
                    ui.horizontal(|ui| {
                        ui.label(model.name);
                        ui.label(RichText::new(text).color(color));
                    });
                    // 只在模型已下載時才有意義：沒下載就談不上 GPU 版
                    if model.state() == ModelState::Ready {
                        match model.gpu_converted() {
                            Some(true) => note(ui, "GPU 版已就緒（去字在 GPU 上跑）"),
                            Some(false) => {
                                note(ui, "GPU 版尚未產生（第一次轉換時自動產生，約 1 秒）")
                            }
                            None => {}
                        }
                    }
                }
                let all_ready = self.ready(Purpose::Erase) && self.ready(Purpose::Objects);
                if !all_ready {
                    ui.horizontal(|ui| {
                        let button = ui.add_enabled(
                            !self.fetching && !self.running,
                            egui::Button::new("下載缺少的模型"),
                        );
                        if self.fetching {
                            ui.spinner();
                            ui.label("下載中…");
                        }
                        if button.clicked() {
                            self.fetch(ui.ctx());
                        }
                    });
                }
                note(
                    ui,
                    "沒有 LaMa 時只擦文字筆畫；沒有 SAM 時物件版只處理平坦底色的頁面。",
                );
            });
    }

    fn queue(&mut self, ui: &mut egui::Ui) {
        if self.rows.is_empty() {
            return;
        }
        section(ui, "處理結果", |ui| {
            for row in &self.rows {
                ui.horizontal(|ui| {
                    let name =
                        format!("{}（{}）", file_name(&row.job.pdf), row.job.variant.label());
                    match &row.status {
                        Status::Waiting => {
                            ui.label(RichText::new("等待").weak());
                            ui.label(name);
                        }
                        Status::Running { fraction, stage } => {
                            ui.spinner();
                            ui.label(name);
                            ui.label(
                                RichText::new(format!("{stage}（{:.0}%）", fraction * 100.0))
                                    .weak(),
                            );
                        }
                        Status::Done {
                            text_boxes,
                            pictures,
                        } => {
                            ui.label(RichText::new("完成").color(OK));
                            ui.label(name);
                            let detail = if row.job.variant == model::Variant::Objects {
                                format!("文字框 {text_boxes}、物件 {pictures}")
                            } else {
                                format!("文字框 {text_boxes}")
                            };
                            ui.label(RichText::new(detail).weak());
                            if ui.small_button("開啟").clicked() {
                                open_file(&row.job.out);
                            }
                            if ui.small_button("在 Finder 顯示").clicked() {
                                reveal_in_finder(&row.job.out);
                            }
                        }
                        Status::Skipped(plan) => {
                            ui.label(RichText::new("跳過").color(WARN));
                            ui.label(name);
                            ui.label(RichText::new(plan.describe()).weak());
                        }
                        Status::Failed(error) => {
                            ui.label(RichText::new("失敗").color(ERROR));
                            ui.label(name);
                            ui.label(RichText::new(error).color(ERROR));
                        }
                        Status::Cancelled => {
                            ui.label(RichText::new("已取消").color(WARN));
                            ui.label(name);
                        }
                    }
                });
            }
        });
    }

    fn actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.running {
            if let Some((fraction, label)) = self.overall_progress() {
                ui.add(egui::ProgressBar::new(fraction).show_percentage());
                ui.label(label);
            }
        } else if let Some((color, text)) = &self.message {
            ui.label(RichText::new(text).color(*color));
        }
        ui.horizontal(|ui| {
            if self.running {
                let cancelling = self.cancel.load(Ordering::Relaxed);
                let text = if cancelling {
                    "取消中…（目前這頁做完後停止）"
                } else {
                    "取消"
                };
                if ui
                    .add_enabled(!cancelling, egui::Button::new(text))
                    .clicked()
                {
                    self.cancel.store(true, Ordering::Relaxed);
                }
            } else {
                let count = self.pdfs.len() * self.mode.variants().len();
                let label = format!("開始轉換（{count} 份）");
                if ui
                    .add_enabled(
                        count > 0 && !self.fetching,
                        egui::Button::new(RichText::new(label).strong()),
                    )
                    .clicked()
                {
                    self.start(ctx);
                }
            }
            ui.label(
                RichText::new("11 頁的簡報：標準版約 0.5–1 分鐘，物件版約 1–1.5 分鐘。").weak(),
            );
        });
    }
}

fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong().size(15.0));
            ui.add_space(4.0);
            body(ui);
        });
}

fn note(ui: &mut egui::Ui, text: &str) {
    ui.indent(text, |ui| {
        ui.label(RichText::new(text).weak().size(12.0));
    });
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 用 macOS 的 `open` 開檔或在 Finder 顯示。
// 走 NSWorkspace 而不是啟動 `open` 子程序：App Sandbox 下子程序繼承沙盒，行為不可靠。
fn pick_output_dir() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("選擇輸出資料夾")
        .pick_folder()
}

/// Finder 顯示的資料夾名稱（例如「下載項目」），不露出含使用者名稱的完整路徑。
fn display_name(path: &std::path::Path) -> String {
    let path = NSString::from_str(&path.to_string_lossy());
    NSFileManager::defaultManager()
        .displayNameAtPath(&path)
        .to_string()
}

fn open_file(path: &std::path::Path) {
    if let Some(url) = NSURL::from_file_path(path) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

fn reveal_in_finder(path: &std::path::Path) {
    if let Some(url) = NSURL::from_file_path(path) {
        NSWorkspace::sharedWorkspace()
            .activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
    }
}

/// egui 內建字型沒有中文。依序找系統內建的中文字型，找到第一個就用。
fn install_cjk_font(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    ];
    let Some(data) = CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("cjk".into(), Arc::new(egui::FontData::from_owned(data)));
    // 放在第一順位：中英文都用同一個字型，基線才會對齊。
    // 等寬字型只當備援（路徑多半是英數字）。
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "cjk".into());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("cjk".into());
    ctx.set_fonts(fonts);
}
