//! 主視窗。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, RichText};
use n2p_image::models::{ModelFile, ModelState, Purpose};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSArray, NSURL};

use crate::debug::DebugHooks;
use crate::i18n::Language;
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
    lang: Language,
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
    pub fn new(cc: &eframe::CreationContext<'_>, mut debug: DebugHooks) -> Self {
        let preset = debug.take_preset();
        let lang = preset.lang.unwrap_or_else(Language::detect);
        install_cjk_font(&cc.egui_ctx, lang);
        let (events_tx, events_rx) = channel();
        let mut app = Self {
            lang,
            pdfs: Vec::new(),
            mode: OutputMode::Standard,
            // 沙盒下拖進來的 PDF 只開放那個檔案、寫不到它所在的資料夾，預設改存「下載項目」
            out_dir: if cfg!(feature = "appstore") {
                model::downloads_dir()
            } else {
                None
            },
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
        let s = self.lang.strings();
        self.message = None;
        if cfg!(feature = "appstore") && self.out_dir.is_none() {
            self.message = Some((ERROR, s.select_output_folder_first.into()));
            return;
        }
        let lama_ready = self.ready(Purpose::Erase);
        let planned = model::plan_jobs(
            &self.pdfs,
            self.mode,
            self.out_dir.as_deref(),
            self.overwrite,
            self.lang,
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
            self.message = Some((WARN, s.nothing_to_convert.into()));
            return;
        }
        self.cancel = Arc::new(AtomicBool::new(false));
        self.running = true;
        let ctx = ctx.clone();
        worker::spawn(
            queue,
            self.cancel.clone(),
            self.events_tx.clone(),
            self.lang,
            move || ctx.request_repaint(),
        );
    }

    fn drain_events(&mut self) {
        let lang = self.lang;
        let s = lang.strings();
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                Event::Started { index } => {
                    self.rows[index].status = Status::Running {
                        fraction: 0.0,
                        stage: s.status_preparing.into(),
                    }
                }
                Event::Progress { index, progress } => {
                    let (fraction, stage) = model::progress_fraction(progress, lang);
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
                        (ERROR, lang.summary_with_failures(done, failed))
                    } else if self.cancel.load(Ordering::Relaxed) {
                        (WARN, lang.summary_cancelled(done))
                    } else {
                        (OK, lang.summary_all_done(done))
                    });
                }
                Event::Fetched(result) => {
                    self.fetching = false;
                    self.refresh_models();
                    self.message = Some(match result {
                        Ok(()) => (OK, s.models_downloaded.into()),
                        Err(error) => (ERROR, lang.model_download_failed(&error)),
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
                    label = self.lang.overall_progress_label(
                        position + 1,
                        active.len(),
                        &file_name(&row.job.pdf),
                        row.job.variant.label(self.lang),
                        stage,
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
                self.message = Some((WARN, self.lang.strings().only_unadded_pdfs.into()));
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
                ui.heading(self.lang.strings().heading);
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
        worker::fetch_models(self.events_tx.clone(), self.lang, move || {
            ctx.request_repaint()
        });
    }

    /// 缺模型時放在最上方：第一次使用的人一定看得到。
    fn first_run(&mut self, ui: &mut egui::Ui) {
        if !self.fetching && !self.missing_models() {
            return;
        }
        let lang = self.lang;
        let s = lang.strings();
        let (done, total) = n2p_image::models::download_progress(&self.models);
        let mb = |bytes: u64| bytes as f64 / 1_000_000.0;
        section(ui, s.first_run_title, |ui| {
            if self.fetching {
                ui.add(
                    egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                        .text(lang.downloading_progress(mb(done), mb(total))),
                );
                // curl 在背景寫檔，定時重畫以更新進度
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(500));
            } else {
                ui.label(lang.first_run_prompt(mb(total - done)));
                if ui
                    .add_enabled(!self.running, egui::Button::new(s.download_models))
                    .clicked()
                {
                    self.fetch(ui.ctx());
                }
                note(ui, s.first_run_note);
            }
        });
        ui.add_space(10.0);
    }

    fn sources(&mut self, ui: &mut egui::Ui, hovering: bool) {
        let s = self.lang.strings();
        section(ui, s.sources_title, |ui| {
            if self.pdfs.is_empty() {
                let text = if hovering {
                    s.drop_to_add
                } else {
                    s.drag_or_add_pdf
                };
                ui.label(RichText::new(text).weak());
            }
            let mut remove = None;
            for (index, pdf) in self.pdfs.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.running, egui::Button::new(s.remove).small())
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
                    if ui.button(s.add_pdf).clicked() {
                        if let Some(files) = rfd::FileDialog::new()
                            .add_filter("PDF", &["pdf", "PDF"])
                            .pick_files()
                        {
                            model::add_pdfs(&mut self.pdfs, files);
                        }
                    }
                    if !self.pdfs.is_empty() && ui.button(s.clear_all).clicked() {
                        self.pdfs.clear();
                        self.rows.clear();
                    }
                });
            });
        });
    }

    fn output_options(&mut self, ui: &mut egui::Ui) {
        let s = self.lang.strings();
        let sam_ready = self.ready(Purpose::Objects);
        section(ui, s.output_version_title, |ui| {
            ui.add_enabled_ui(!self.running, |ui| {
                ui.radio_value(&mut self.mode, OutputMode::Standard, s.standard_recommended);
                note(ui, s.standard_note);
                ui.radio_value(&mut self.mode, OutputMode::Objects, s.objects_experimental);
                note(ui, s.objects_note);
                ui.radio_value(&mut self.mode, OutputMode::Both, s.generate_both);
            });
            if self.mode.needs_sam() && !sam_ready {
                ui.label(RichText::new(s.missing_sam_warning).color(WARN));
            }
        });

        section(ui, s.output_location_title, |ui| {
            ui.add_enabled_ui(!self.running, |ui| {
                if cfg!(feature = "appstore") {
                    self.sandboxed_output_dir(ui);
                } else {
                    self.output_dir(ui);
                }
                ui.checkbox(&mut self.overwrite, s.overwrite_existing);
                note(ui, s.powerpoint_open_note);
            });
        });
    }

    /// App Store 版：只有「下載項目」與使用者在面板裡選的資料夾可寫（沙盒）。
    fn sandboxed_output_dir(&mut self, ui: &mut egui::Ui) {
        let s = self.lang.strings();
        ui.horizontal(|ui| {
            ui.label(s.save_to);
            let shown = match &self.out_dir {
                Some(dir) if model::is_downloads(dir) => s.downloads_folder.to_owned(),
                Some(dir) => dir.display().to_string(),
                None => s.not_selected.to_owned(),
            };
            ui.add(egui::Label::new(RichText::new(shown).monospace()).truncate());
            if ui.button(s.change_folder).clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.out_dir = Some(dir);
                }
            }
        });
    }

    /// 一般版：預設與 PDF 同資料夾，也可指定資料夾。
    fn output_dir(&mut self, ui: &mut egui::Ui) {
        let s = self.lang.strings();
        ui.horizontal(|ui| {
            let mut custom = self.out_dir.is_some();
            ui.radio_value(&mut custom, false, s.same_folder_as_pdf);
            ui.radio_value(&mut custom, true, s.custom_folder);
            if custom && self.out_dir.is_none() {
                self.out_dir = rfd::FileDialog::new().pick_folder();
            } else if !custom {
                self.out_dir = None;
            }
        });
        if let Some(shown) = self.out_dir.as_ref().map(|d| d.display().to_string()) {
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(shown).monospace()).truncate());
                if ui.button(s.change_folder).clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.out_dir = Some(dir);
                    }
                }
            });
        }
    }

    fn advanced(&mut self, ui: &mut egui::Ui) {
        let s = self.lang.strings();
        // 固定 id_salt：切換語言時標題文字改變，不重設展開／收合狀態
        egui::CollapsingHeader::new(s.advanced_settings)
            .id_salt("advanced_settings")
            .default_open(self.advanced_open)
            .show(ui, |ui| {
                ui.add_enabled_ui(!self.running, |ui| {
                    let prev_lang = self.lang;
                    ui.horizontal(|ui| {
                        ui.label(s.ui_language);
                        egui::ComboBox::from_id_salt("ui_language")
                            .selected_text(self.lang.display_name())
                            .show_ui(ui, |ui| {
                                for &candidate in Language::ALL {
                                    ui.selectable_value(
                                        &mut self.lang,
                                        candidate,
                                        candidate.display_name(),
                                    );
                                }
                            });
                    });
                    if self.lang != prev_lang {
                        install_cjk_font(ui.ctx(), self.lang);
                    }
                    let s = self.lang.strings();
                    ui.checkbox(
                        &mut self.settings.high_res_background,
                        s.high_res_background,
                    );
                    ui.checkbox(&mut self.settings.widescreen, s.pad_to_16x9);
                    ui.horizontal(|ui| {
                        ui.label(s.page_range);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings.pages)
                                .hint_text(s.page_range_hint)
                                .desired_width(160.0),
                        );
                    });
                });
                let lang = self.lang;
                let s = lang.strings();
                ui.add_space(6.0);
                ui.label(RichText::new(s.models_heading).strong());
                if let Some(error) = &self.models_error {
                    ui.label(RichText::new(error).color(ERROR));
                }
                for model in &self.models {
                    let (color, text) = match model.state() {
                        ModelState::Ready => (OK, s.model_ready.to_owned()),
                        ModelState::Missing => (WARN, s.model_missing.to_owned()),
                        ModelState::Partial { size } => {
                            (WARN, lang.model_partial(size, model.expected_size))
                        }
                    };
                    ui.horizontal(|ui| {
                        ui.label(lang.model_name(model));
                        ui.label(RichText::new(text).color(color));
                    });
                    // 只在模型已下載時才有意義：沒下載就談不上 GPU 版
                    if model.state() == ModelState::Ready {
                        match model.gpu_converted() {
                            Some(true) => note(ui, s.gpu_ready_note),
                            Some(false) => note(ui, s.gpu_pending_note),
                            None => {}
                        }
                    }
                }
                let all_ready = self.ready(Purpose::Erase) && self.ready(Purpose::Objects);
                if !all_ready {
                    ui.horizontal(|ui| {
                        let button = ui.add_enabled(
                            !self.fetching && !self.running,
                            egui::Button::new(s.download_missing_models),
                        );
                        if self.fetching {
                            ui.spinner();
                            ui.label(s.downloading);
                        }
                        if button.clicked() {
                            self.fetch(ui.ctx());
                        }
                    });
                }
                note(ui, s.models_fallback_note);
            });
    }

    fn queue(&mut self, ui: &mut egui::Ui) {
        if self.rows.is_empty() {
            return;
        }
        let lang = self.lang;
        let s = lang.strings();
        section(ui, s.results_title, |ui| {
            for row in &self.rows {
                ui.horizontal(|ui| {
                    let name = lang
                        .job_display_name(&file_name(&row.job.pdf), row.job.variant.label(lang));
                    match &row.status {
                        Status::Waiting => {
                            ui.label(RichText::new(s.status_waiting).weak());
                            ui.label(name);
                        }
                        Status::Running { fraction, stage } => {
                            ui.spinner();
                            ui.label(name);
                            ui.label(
                                RichText::new(lang.running_stage_percent(stage, fraction * 100.0))
                                    .weak(),
                            );
                        }
                        Status::Done {
                            text_boxes,
                            pictures,
                        } => {
                            ui.label(RichText::new(s.status_done).color(OK));
                            ui.label(name);
                            let pics =
                                (row.job.variant == model::Variant::Objects).then_some(*pictures);
                            let detail = lang.done_detail(*text_boxes, pics);
                            ui.label(RichText::new(detail).weak());
                            if ui.small_button(s.open_file).clicked() {
                                open_file(&row.job.out);
                            }
                            if ui.small_button(s.reveal_in_finder).clicked() {
                                reveal_in_finder(&row.job.out);
                            }
                        }
                        Status::Skipped(plan) => {
                            ui.label(RichText::new(s.status_skipped).color(WARN));
                            ui.label(name);
                            ui.label(RichText::new(plan.describe(lang)).weak());
                        }
                        Status::Failed(error) => {
                            ui.label(RichText::new(s.status_failed).color(ERROR));
                            ui.label(name);
                            ui.label(RichText::new(error).color(ERROR));
                        }
                        Status::Cancelled => {
                            ui.label(RichText::new(s.status_cancelled).color(WARN));
                            ui.label(name);
                        }
                    }
                });
            }
        });
    }

    fn actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let s = self.lang.strings();
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
                let text = if cancelling { s.cancelling } else { s.cancel };
                if ui
                    .add_enabled(!cancelling, egui::Button::new(text))
                    .clicked()
                {
                    self.cancel.store(true, Ordering::Relaxed);
                }
            } else {
                let count = self.pdfs.len() * self.mode.variants().len();
                let label = self.lang.start_conversion(count);
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
            ui.label(RichText::new(s.speed_estimate_note).weak());
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

/// egui 內建字型沒有中日韓字形。依介面語言優先順序載入系統內建字型，並保留備援字型以涵蓋全部六語字元。
fn install_cjk_font(ctx: &egui::Context, lang: Language) {
    const JA_CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    ];
    const ZH_CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    ];
    let groups: [&[&str]; 2] = if lang == Language::Ja {
        [JA_CANDIDATES, ZH_CANDIDATES]
    } else {
        [ZH_CANDIDATES, JA_CANDIDATES]
    };

    let mut fonts = egui::FontDefinitions::default();
    let mut loaded = Vec::new();
    for (idx, candidates) in groups.iter().enumerate() {
        if let Some(data) = candidates.iter().find_map(|p| std::fs::read(p).ok()) {
            let key = format!("cjk_{idx}");
            fonts
                .font_data
                .insert(key.clone(), Arc::new(egui::FontData::from_owned(data)));
            loaded.push(key);
        }
    }
    if loaded.is_empty() {
        return;
    }
    // 放在第一順位：中日韓與拉丁字元都優先走系統字型，基線才會對齊；
    // 同時掛上第二順位字型，讓語言選單同時顯示「繁體中文／简体中文／日本語」也不缺字。
    let proportional = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    for (pos, key) in loaded.iter().enumerate() {
        proportional.insert(pos, key.clone());
    }
    let monospace = fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default();
    for key in loaded {
        monospace.push(key);
    }
    ctx.set_fonts(fonts);
}
