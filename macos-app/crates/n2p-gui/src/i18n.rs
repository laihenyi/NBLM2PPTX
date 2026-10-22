//! 多語言介面文字與系統語言偵測（支援繁中、簡中、英、日、西、法六語）。

use std::path::Path;

use n2p_image::models::{ModelFile, Purpose};

/// 介面支援的語言（與根目錄六語 README 一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    ZhTw,
    ZhCn,
    En,
    Ja,
    Es,
    Fr,
}

impl Language {
    pub const ALL: &[Language] = &[
        Language::ZhTw,
        Language::ZhCn,
        Language::En,
        Language::Ja,
        Language::Es,
        Language::Fr,
    ];

    /// BCP-47 語言代碼（亦供 `N2P_GUI_LANG` 使用）。
    #[cfg(test)]
    pub fn code(self) -> &'static str {
        match self {
            Language::ZhTw => "zh-TW",
            Language::ZhCn => "zh-CN",
            Language::En => "en",
            Language::Ja => "ja",
            Language::Es => "es",
            Language::Fr => "fr",
        }
    }

    /// 語言選單中顯示的自稱（以各該語言書寫，確保誤切時仍能認得）。
    pub fn display_name(self) -> &'static str {
        match self {
            Language::ZhTw => "繁體中文",
            Language::ZhCn => "简体中文",
            Language::En => "English",
            Language::Ja => "日本語",
            Language::Es => "Español",
            Language::Fr => "Français",
        }
    }

    /// 解析 BCP-47 或 POSIX locale 字串（例如 `ja-JP`、`zh_TW.UTF-8`、`zh-Hans-CN`）。
    pub fn from_tag(tag: &str) -> Option<Self> {
        let base = tag
            .trim()
            .split(['.', '@'])
            .next()
            .unwrap_or("")
            .replace('_', "-")
            .to_ascii_lowercase();
        if base.is_empty() {
            return None;
        }
        let mut parts = base.split('-');
        let primary = parts.next()?;
        let rest: Vec<&str> = parts.collect();
        match primary {
            "zh" => {
                if rest.iter().any(|&s| s == "hans" || s == "cn" || s == "sg") {
                    Some(Language::ZhCn)
                } else {
                    Some(Language::ZhTw)
                }
            }
            "ja" | "jp" => Some(Language::Ja),
            "en" => Some(Language::En),
            "es" => Some(Language::Es),
            "fr" => Some(Language::Fr),
            _ => None,
        }
    }

    /// 依 macOS 系統偏好語言（`NSLocale.preferredLanguages`）與環境變數自動決定介面語言。
    pub fn detect() -> Self {
        for tag in macos_preferred_languages() {
            if let Some(lang) = Self::from_tag(&tag) {
                return lang;
            }
        }
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                if let Some(lang) = Self::from_tag(&val) {
                    return lang;
                }
            }
        }
        Language::En
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Language::ZhTw => &ZH_TW,
            Language::ZhCn => &ZH_CN,
            Language::En => &EN,
            Language::Ja => &JA,
            Language::Es => &ES,
            Language::Fr => &FR,
        }
    }

    pub fn first_run_prompt(self, remaining_mb: f64) -> String {
        match self {
            Language::ZhTw => {
                format!("去字與物件切出需要 AI 模型，只需下載一次（還差約 {remaining_mb:.0} MB）。")
            }
            Language::ZhCn => {
                format!("去字与对象提取需要 AI 模型，只需下载一次（还差约 {remaining_mb:.0} MB）。")
            }
            Language::En => format!(
                "AI models are required for text removal and object extraction (one-time download, ~{remaining_mb:.0} MB remaining)."
            ),
            Language::Ja => format!(
                "文字消去とオブジェクト切り出しには AI モデルが必要です。初回のみダウンロードします（残り約 {remaining_mb:.0} MB）。"
            ),
            Language::Es => format!(
                "Se necesitan modelos de IA para eliminar texto y extraer objetos (descarga única, faltan ~{remaining_mb:.0} MB)."
            ),
            Language::Fr => format!(
                "Des modèles d’IA sont nécessaires pour supprimer le texte et extraire les objets (téléchargement unique, ~{remaining_mb:.0} Mo restants)."
            ),
        }
    }

    pub fn downloading_progress(self, done_mb: f64, total_mb: f64) -> String {
        match self {
            Language::ZhTw => format!("下載中… {done_mb:.0} / {total_mb:.0} MB"),
            Language::ZhCn => format!("下载中… {done_mb:.0} / {total_mb:.0} MB"),
            Language::En => format!("Downloading… {done_mb:.0} / {total_mb:.0} MB"),
            Language::Ja => format!("ダウンロード中… {done_mb:.0} / {total_mb:.0} MB"),
            Language::Es => format!("Descargando… {done_mb:.0} / {total_mb:.0} MB"),
            Language::Fr => format!("Téléchargement… {done_mb:.0} / {total_mb:.0} Mo"),
        }
    }

    pub fn model_partial(self, size: u64, expected: u64) -> String {
        match self {
            Language::ZhTw => format!("不完整（{size} / {expected} 位元組）"),
            Language::ZhCn => format!("不完整（{size} / {expected} 字节）"),
            Language::En => format!("Incomplete ({size} / {expected} bytes)"),
            Language::Ja => format!("不完全（{size} / {expected} バイト）"),
            Language::Es => format!("Incompleto ({size} / {expected} bytes)"),
            Language::Fr => format!("Incomplet ({size} / {expected} octets)"),
        }
    }

    pub fn model_download_failed(self, error: &str) -> String {
        match self {
            Language::ZhTw => format!("模型下載失敗：{error}"),
            Language::ZhCn => format!("模型下载失败：{error}"),
            Language::En => format!("Model download failed: {error}"),
            Language::Ja => format!("モデルのダウンロードに失敗しました：{error}"),
            Language::Es => format!("Error al descargar los modelos: {error}"),
            Language::Fr => format!("Échec du téléchargement des modèles : {error}"),
        }
    }

    pub fn start_conversion(self, count: usize) -> String {
        match self {
            Language::ZhTw => format!("開始轉換（{count} 份）"),
            Language::ZhCn => format!("开始转换（{count} 份）"),
            Language::En => format!("Start Conversion ({count})"),
            Language::Ja => format!("変換開始（{count} 件）"),
            Language::Es => format!("Iniciar conversión ({count})"),
            Language::Fr => format!("Démarrer la conversion ({count})"),
        }
    }

    pub fn overall_progress_label(
        self,
        current: usize,
        total: usize,
        file: &str,
        variant: &str,
        stage: &str,
    ) -> String {
        match self {
            Language::ZhTw => format!("第 {current}/{total} 份：{file}（{variant}）— {stage}"),
            Language::ZhCn => format!("第 {current}/{total} 份：{file}（{variant}）— {stage}"),
            Language::En => format!("{current}/{total}: {file} ({variant}) — {stage}"),
            Language::Ja => format!("{current}/{total} 件目：{file}（{variant}）— {stage}"),
            Language::Es => format!("{current}/{total}: {file} ({variant}) — {stage}"),
            Language::Fr => format!("{current}/{total} : {file} ({variant}) — {stage}"),
        }
    }

    pub fn running_stage_percent(self, stage: &str, percent: f32) -> String {
        match self {
            Language::ZhTw | Language::ZhCn | Language::Ja => {
                format!("{stage}（{percent:.0}%）")
            }
            Language::En | Language::Es | Language::Fr => {
                format!("{stage} ({percent:.0}%)")
            }
        }
    }

    pub fn job_display_name(self, file: &str, variant: &str) -> String {
        match self {
            Language::ZhTw | Language::ZhCn | Language::Ja => format!("{file}（{variant}）"),
            Language::En | Language::Es | Language::Fr => format!("{file} ({variant})"),
        }
    }

    pub fn done_detail(self, text_boxes: usize, pictures: Option<usize>) -> String {
        match (self, pictures) {
            (Language::ZhTw, Some(pics)) => format!("文字框 {text_boxes}、物件 {pics}"),
            (Language::ZhTw, None) => format!("文字框 {text_boxes}"),
            (Language::ZhCn, Some(pics)) => format!("文本框 {text_boxes}、对象 {pics}"),
            (Language::ZhCn, None) => format!("文本框 {text_boxes}"),
            (Language::En, Some(pics)) => format!("text boxes: {text_boxes}, objects: {pics}"),
            (Language::En, None) => format!("text boxes: {text_boxes}"),
            (Language::Ja, Some(pics)) => {
                format!("テキストボックス {text_boxes}、オブジェクト {pics}")
            }
            (Language::Ja, None) => format!("テキストボックス {text_boxes}"),
            (Language::Es, Some(pics)) => {
                format!("cuadros de texto: {text_boxes}, objetos: {pics}")
            }
            (Language::Es, None) => format!("cuadros de texto: {text_boxes}"),
            (Language::Fr, Some(pics)) => format!("zones de texte : {text_boxes}, objets : {pics}"),
            (Language::Fr, None) => format!("zones de texte : {text_boxes}"),
        }
    }

    pub fn summary_all_done(self, done: usize) -> String {
        match self {
            Language::ZhTw => format!("全部完成，共 {done} 份。"),
            Language::ZhCn => format!("全部完成，共 {done} 份。"),
            Language::En => format!("All done ({done} completed)."),
            Language::Ja => format!("すべて完了しました（計 {done} 件）。"),
            Language::Es => format!("Todo listo ({done} completados)."),
            Language::Fr => format!("Terminé ({done} réalisés)."),
        }
    }

    pub fn summary_cancelled(self, done: usize) -> String {
        match self {
            Language::ZhTw => format!("已取消。完成 {done} 份。"),
            Language::ZhCn => format!("已取消。完成 {done} 份。"),
            Language::En => format!("Cancelled. Completed {done}."),
            Language::Ja => format!("キャンセルしました（完了 {done} 件）。"),
            Language::Es => format!("Cancelado. Completados: {done}."),
            Language::Fr => format!("Annulé. Terminés : {done}."),
        }
    }

    pub fn summary_with_failures(self, done: usize, failed: usize) -> String {
        match self {
            Language::ZhTw => format!("完成 {done} 份，失敗 {failed} 份。"),
            Language::ZhCn => format!("完成 {done} 份，失败 {failed} 份。"),
            Language::En => format!("Completed {done}, failed {failed}."),
            Language::Ja => format!("完了 {done} 件、失敗 {failed} 件。"),
            Language::Es => format!("Completados: {done}, fallidos: {failed}."),
            Language::Fr => format!("Terminés : {done}, échoués : {failed}."),
        }
    }

    pub fn stage_render(self, page: usize, total: usize) -> String {
        match self {
            Language::ZhTw => format!("讀取頁面 {page}/{total}"),
            Language::ZhCn => format!("读取页面 {page}/{total}"),
            Language::En => format!("Reading page {page}/{total}"),
            Language::Ja => format!("ページ読み込み中 {page}/{total}"),
            Language::Es => format!("Leyendo página {page}/{total}"),
            Language::Fr => format!("Lecture de la page {page}/{total}"),
        }
    }

    pub fn stage_build(self, page: usize, total: usize) -> String {
        match self {
            Language::ZhTw => format!("產生投影片 {page}/{total}"),
            Language::ZhCn => format!("生成幻灯片 {page}/{total}"),
            Language::En => format!("Building slide {page}/{total}"),
            Language::Ja => format!("スライド作成中 {page}/{total}"),
            Language::Es => format!("Generando diapositiva {page}/{total}"),
            Language::Fr => format!("Création de la diapositive {page}/{total}"),
        }
    }

    pub fn write_error(self, path: &Path, error: &std::io::Error) -> String {
        match self {
            Language::ZhTw => format!("無法寫入 {}：{error}", path.display()),
            Language::ZhCn => format!("无法写入 {}：{error}", path.display()),
            Language::En => format!("Failed to write {}: {error}", path.display()),
            Language::Ja => format!("{} に書き込めません：{error}", path.display()),
            Language::Es => format!("No se pudo escribir {}: {error}", path.display()),
            Language::Fr => format!("Impossible d’écrire {} : {error}", path.display()),
        }
    }

    pub fn model_name(self, model: &ModelFile) -> &'static str {
        let s = self.strings();
        match model.purpose {
            Purpose::Erase => s.model_lama_name,
            Purpose::Objects => {
                let is_encoder = model
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.contains("encoder"));
                if is_encoder {
                    s.model_sam_encoder_name
                } else {
                    s.model_sam_decoder_name
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_preferred_languages() -> Vec<String> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyClass;
    use objc2_foundation::{NSArray, NSString};

    let Some(cls) = AnyClass::get(c"NSLocale") else {
        return Vec::new();
    };
    let langs: Retained<NSArray<NSString>> = unsafe { msg_send![cls, preferredLanguages] };
    (0..langs.count())
        .map(|i| langs.objectAtIndex(i).to_string())
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn macos_preferred_languages() -> Vec<String> {
    Vec::new()
}

/// 靜態介面文字表。
pub struct Strings {
    pub heading: &'static str,
    pub first_run_title: &'static str,
    pub download_models: &'static str,
    pub first_run_note: &'static str,
    pub sources_title: &'static str,
    pub drop_to_add: &'static str,
    pub drag_or_add_pdf: &'static str,
    pub only_unadded_pdfs: &'static str,
    pub remove: &'static str,
    pub add_pdf: &'static str,
    pub clear_all: &'static str,
    pub output_version_title: &'static str,
    pub standard_recommended: &'static str,
    pub standard_note: &'static str,
    pub objects_experimental: &'static str,
    pub objects_note: &'static str,
    pub generate_both: &'static str,
    pub missing_sam_warning: &'static str,
    pub output_location_title: &'static str,
    pub same_folder_as_pdf: &'static str,
    pub custom_folder: &'static str,
    pub save_to: &'static str,
    pub downloads_folder: &'static str,
    pub not_selected: &'static str,
    pub select_output_folder_first: &'static str,
    pub change_folder: &'static str,
    pub overwrite_existing: &'static str,
    pub powerpoint_open_note: &'static str,
    pub advanced_settings: &'static str,
    pub ui_language: &'static str,
    pub high_res_background: &'static str,
    pub pad_to_16x9: &'static str,
    pub page_range: &'static str,
    pub page_range_hint: &'static str,
    pub models_heading: &'static str,
    pub model_ready: &'static str,
    pub model_missing: &'static str,
    pub gpu_ready_note: &'static str,
    pub gpu_pending_note: &'static str,
    pub download_missing_models: &'static str,
    pub downloading: &'static str,
    pub models_fallback_note: &'static str,
    pub models_downloaded: &'static str,
    pub results_title: &'static str,
    pub status_waiting: &'static str,
    pub status_preparing: &'static str,
    pub status_done: &'static str,
    pub status_skipped: &'static str,
    pub status_failed: &'static str,
    pub status_cancelled: &'static str,
    pub open_file: &'static str,
    pub reveal_in_finder: &'static str,
    pub nothing_to_convert: &'static str,
    pub cancel: &'static str,
    pub cancelling: &'static str,
    pub speed_estimate_note: &'static str,
    pub variant_standard_label: &'static str,
    pub variant_objects_label: &'static str,
    pub variant_standard_suffix: &'static str,
    pub variant_objects_suffix: &'static str,
    pub plan_convert: &'static str,
    pub plan_skip_exists: &'static str,
    pub plan_skip_locked: &'static str,
    pub stage_layout: &'static str,
    pub stage_save: &'static str,
    pub activity_converting: &'static str,
    pub activity_downloading: &'static str,
    pub model_lama_name: &'static str,
    pub model_sam_encoder_name: &'static str,
    pub model_sam_decoder_name: &'static str,
}

const ZH_TW: Strings = Strings {
    heading: "NotebookLM PDF → 可編輯 PowerPoint",
    first_run_title: "首次使用：下載模型",
    download_models: "下載模型",
    first_run_note: "不下載也能轉換：標準版只擦文字筆畫，品質較差；物件版只處理平坦底色的頁面。",
    sources_title: "1. 來源 PDF",
    drop_to_add: "放開以加入",
    drag_or_add_pdf: "把 PDF 拖進視窗，或按「加入 PDF…」",
    only_unadded_pdfs: "只接受尚未加入的 PDF 檔。",
    remove: "移除",
    add_pdf: "加入 PDF…",
    clear_all: "全部清除",
    output_version_title: "2. 輸出版本",
    standard_recommended: "標準版（建議）",
    standard_note: "文字變成可編輯的文字框，圖片留在背景。檔名結尾「_可編輯」。",
    objects_experimental: "物件版（實驗性）",
    objects_note: "另把圖示、插圖切成可移動的圖片。移走物件後，原位是補出來的背景，可能有暗塊。檔名結尾「_可編輯_物件」。",
    generate_both: "兩種都產生",
    missing_sam_warning: "物件版需要 SAM 模型。目前缺少，只有平坦底色的頁面能切出物件。請先下載模型。",
    output_location_title: "3. 輸出位置",
    same_folder_as_pdf: "與 PDF 相同的資料夾",
    custom_folder: "指定資料夾",
    save_to: "存到",
    downloads_folder: "下載項目",
    not_selected: "（尚未選擇）",
    select_output_folder_first: "請先選擇輸出資料夾。",
    change_folder: "更改…",
    overwrite_existing: "覆寫已存在的檔案",
    powerpoint_open_note: "PowerPoint 正開著的檔案一律不覆寫。",
    advanced_settings: "進階設定",
    ui_language: "介面語言",
    high_res_background: "高解析度背景（較清晰，檔案較大）",
    pad_to_16x9: "把頁面補成 16:9",
    page_range: "頁碼範圍",
    page_range_hint: "全部（例：1-3,5）",
    models_heading: "模型",
    model_ready: "已就緒",
    model_missing: "未下載",
    gpu_ready_note: "GPU 版已就緒（去字在 GPU 上跑）",
    gpu_pending_note: "GPU 版尚未產生（第一次轉換時自動產生，約 1 秒）",
    download_missing_models: "下載缺少的模型",
    downloading: "下載中…",
    models_fallback_note: "沒有 LaMa 時只擦文字筆畫；沒有 SAM 時物件版只處理平坦底色的頁面。",
    models_downloaded: "模型已下載完成。",
    results_title: "處理結果",
    status_waiting: "等待",
    status_preparing: "準備中",
    status_done: "完成",
    status_skipped: "跳過",
    status_failed: "失敗",
    status_cancelled: "已取消",
    open_file: "開啟",
    reveal_in_finder: "在 Finder 顯示",
    nothing_to_convert: "沒有需要轉換的檔案。",
    cancel: "取消",
    cancelling: "取消中…（目前這頁做完後停止）",
    speed_estimate_note: "11 頁的簡報：標準版約 0.5–1 分鐘，物件版約 1–1.5 分鐘。",
    variant_standard_label: "標準版",
    variant_objects_label: "物件版",
    variant_standard_suffix: "_可編輯",
    variant_objects_suffix: "_可編輯_物件",
    plan_convert: "轉換",
    plan_skip_exists: "已存在，跳過（要重做請勾選「覆寫已存在的檔案」）",
    plan_skip_locked: "PowerPoint 正開著這個檔案，跳過（請先關閉）",
    stage_layout: "辨識文字",
    stage_save: "寫出檔案",
    activity_converting: "正在轉換 PDF",
    activity_downloading: "正在下載模型",
    model_lama_name: "LaMa 模型（去字修補，約 199 MiB）",
    model_sam_encoder_name: "SAM encoder（物件切出，約 27 MiB）",
    model_sam_decoder_name: "SAM decoder（物件切出，約 16 MiB）",
};

const ZH_CN: Strings = Strings {
    heading: "NotebookLM PDF → 可编辑 PowerPoint",
    first_run_title: "首次使用：下载模型",
    download_models: "下载模型",
    first_run_note: "不下载也能转换：标准版只擦除文字笔画，品质较差；对象版只处理平坦底色的页面。",
    sources_title: "1. 来源 PDF",
    drop_to_add: "放开以添加",
    drag_or_add_pdf: "把 PDF 拖进窗口，或按“添加 PDF…”",
    only_unadded_pdfs: "只接受尚未添加的 PDF 文件。",
    remove: "移除",
    add_pdf: "添加 PDF…",
    clear_all: "全部清除",
    output_version_title: "2. 输出版本",
    standard_recommended: "标准版（建议）",
    standard_note: "文字变成可编辑的文本框，图片留在背景。文件名结尾“_可编辑”。",
    objects_experimental: "对象版（实验性）",
    objects_note: "另把图标、插图切成可移动的图片。移走对象后，原位是补出来的背景，可能有暗块。文件名结尾“_可编辑_对象”。",
    generate_both: "两种都生成",
    missing_sam_warning: "对象版需要 SAM 模型。目前缺少，只有平坦底色的页面能提取对象。请先下载模型。",
    output_location_title: "3. 输出位置",
    same_folder_as_pdf: "与 PDF 相同的文件夹",
    custom_folder: "指定文件夹",
    save_to: "存到",
    downloads_folder: "下载",
    not_selected: "（尚未选择）",
    select_output_folder_first: "请先选择输出文件夹。",
    change_folder: "更改…",
    overwrite_existing: "覆盖已存在的文件",
    powerpoint_open_note: "PowerPoint 正开着的文件一律不覆盖。",
    advanced_settings: "高级设置",
    ui_language: "界面语言",
    high_res_background: "高分辨率背景（较清晰，文件较大）",
    pad_to_16x9: "把页面补成 16:9",
    page_range: "页码范围",
    page_range_hint: "全部（例：1-3,5）",
    models_heading: "模型",
    model_ready: "已就绪",
    model_missing: "未下载",
    gpu_ready_note: "GPU 版已就绪（去字在 GPU 上运行）",
    gpu_pending_note: "GPU 版尚未生成（第一次转换时自动生成，约 1 秒）",
    download_missing_models: "下载缺少的模型",
    downloading: "下载中…",
    models_fallback_note: "没有 LaMa 时只擦除文字笔画；没有 SAM 时对象版只处理平坦底色的页面。",
    models_downloaded: "模型已下载完成。",
    results_title: "处理结果",
    status_waiting: "等待",
    status_preparing: "准备中",
    status_done: "完成",
    status_skipped: "跳过",
    status_failed: "失败",
    status_cancelled: "已取消",
    open_file: "打开",
    reveal_in_finder: "在访达中显示",
    nothing_to_convert: "没有需要转换的文件。",
    cancel: "取消",
    cancelling: "取消中…（当前页完成后停止）",
    speed_estimate_note: "11 页的演示文稿：标准版约 0.5–1 分钟，对象版约 1–1.5 分钟。",
    variant_standard_label: "标准版",
    variant_objects_label: "对象版",
    variant_standard_suffix: "_可编辑",
    variant_objects_suffix: "_可编辑_对象",
    plan_convert: "转换",
    plan_skip_exists: "已存在，跳过（要重做请勾选“覆盖已存在的文件”）",
    plan_skip_locked: "PowerPoint 正开着这个文件，跳过（请先关闭）",
    stage_layout: "识别文字",
    stage_save: "写入文件",
    activity_converting: "正在转换 PDF",
    activity_downloading: "正在下载模型",
    model_lama_name: "LaMa 模型（去字修补，约 199 MiB）",
    model_sam_encoder_name: "SAM encoder（对象提取，约 27 MiB）",
    model_sam_decoder_name: "SAM decoder（对象提取，约 16 MiB）",
};

const EN: Strings = Strings {
    heading: "NotebookLM PDF → Editable PowerPoint",
    first_run_title: "First Launch: Download Models",
    download_models: "Download Models",
    first_run_note: "You can still convert without downloading: Standard only erases text strokes (lower quality); Objects only processes flat-background pages.",
    sources_title: "1. Source PDF",
    drop_to_add: "Drop to add",
    drag_or_add_pdf: "Drag PDFs into the window, or click \"Add PDF…\"",
    only_unadded_pdfs: "Only PDF files not already in the list are accepted.",
    remove: "Remove",
    add_pdf: "Add PDF…",
    clear_all: "Clear All",
    output_version_title: "2. Output Version",
    standard_recommended: "Standard (recommended)",
    standard_note: "Text becomes editable text boxes; images stay in the background. Filename ends with \"_editable\".",
    objects_experimental: "Objects (experimental)",
    objects_note: "Also extracts icons and illustrations as movable images. Moving an object reveals an inpainted background that may have dark patches. Filename ends with \"_editable_objects\".",
    generate_both: "Generate Both",
    missing_sam_warning: "Objects version requires the SAM model. Currently missing, so only flat-background pages can extract objects. Please download the models first.",
    output_location_title: "3. Output Location",
    same_folder_as_pdf: "Same folder as PDF",
    custom_folder: "Custom folder",
    save_to: "Save to",
    downloads_folder: "Downloads",
    not_selected: "(not selected)",
    select_output_folder_first: "Please select an output folder first.",
    change_folder: "Change…",
    overwrite_existing: "Overwrite existing files",
    powerpoint_open_note: "Files currently open in PowerPoint are never overwritten.",
    advanced_settings: "Advanced Settings",
    ui_language: "Language",
    high_res_background: "High-resolution background (sharper, larger file)",
    pad_to_16x9: "Pad pages to 16:9",
    page_range: "Page range",
    page_range_hint: "All (e.g. 1-3,5)",
    models_heading: "Models",
    model_ready: "Ready",
    model_missing: "Not downloaded",
    gpu_ready_note: "GPU version ready (text removal runs on GPU)",
    gpu_pending_note: "GPU version not built yet (generated automatically on first conversion, ~1 s)",
    download_missing_models: "Download Missing Models",
    downloading: "Downloading…",
    models_fallback_note: "Without LaMa, only text strokes are erased; without SAM, Objects version only processes flat-background pages.",
    models_downloaded: "Models downloaded successfully.",
    results_title: "Results",
    status_waiting: "Waiting",
    status_preparing: "Preparing",
    status_done: "Done",
    status_skipped: "Skipped",
    status_failed: "Failed",
    status_cancelled: "Cancelled",
    open_file: "Open",
    reveal_in_finder: "Show in Finder",
    nothing_to_convert: "No files to convert.",
    cancel: "Cancel",
    cancelling: "Cancelling… (stops after current page)",
    speed_estimate_note: "11-page deck: ~0.5–1 min for Standard, ~1–1.5 min for Objects.",
    variant_standard_label: "Standard",
    variant_objects_label: "Objects",
    variant_standard_suffix: "_editable",
    variant_objects_suffix: "_editable_objects",
    plan_convert: "Convert",
    plan_skip_exists: "Already exists, skipped (check \"Overwrite existing files\" to redo)",
    plan_skip_locked: "Currently open in PowerPoint, skipped (please close it first)",
    stage_layout: "Recognizing text",
    stage_save: "Writing file",
    activity_converting: "Converting PDF",
    activity_downloading: "Downloading models",
    model_lama_name: "LaMa model (text inpainting, ~199 MiB)",
    model_sam_encoder_name: "SAM encoder (object extraction, ~27 MiB)",
    model_sam_decoder_name: "SAM decoder (object extraction, ~16 MiB)",
};

const JA: Strings = Strings {
    heading: "NotebookLM PDF → 編集可能な PowerPoint",
    first_run_title: "初回利用：モデルのダウンロード",
    download_models: "モデルをダウンロード",
    first_run_note: "ダウンロードしなくても変換できます：標準版は文字のストロークのみを消去するため品質が下がり、オブジェクト版は単色背景のページのみ処理します。",
    sources_title: "1. 変換元の PDF",
    drop_to_add: "ドロップして追加",
    drag_or_add_pdf: "PDF をウィンドウにドラッグ＆ドロップするか、「PDF を追加…」を押してください",
    only_unadded_pdfs: "まだ追加されていない PDF ファイルのみ追加できます。",
    remove: "削除",
    add_pdf: "PDF を追加…",
    clear_all: "すべてクリア",
    output_version_title: "2. 出力バージョン",
    standard_recommended: "標準版（推奨）",
    standard_note: "文字は編集可能なテキストボックスになり、画像は背景に残ります。ファイル名末尾は「_編集可能」。",
    objects_experimental: "オブジェクト版（実験的）",
    objects_note: "アイコンやイラストも移動可能な画像に切り出します。移動した跡は補完された背景になるため、色むらが出ることがあります。ファイル名末尾は「_編集可能_オブジェクト」。",
    generate_both: "両方を作成",
    missing_sam_warning: "オブジェクト版には SAM モデルが必要です。現在未ダウンロードのため、単色背景のページのみオブジェクトを切り出せます。先にモデルをダウンロードしてください。",
    output_location_title: "3. 出力先",
    same_folder_as_pdf: "PDF と同じフォルダ",
    custom_folder: "フォルダを指定",
    save_to: "保存先",
    downloads_folder: "ダウンロード",
    not_selected: "（未選択）",
    select_output_folder_first: "先に出力先フォルダを選択してください。",
    change_folder: "変更…",
    overwrite_existing: "既存のファイルを上書き",
    powerpoint_open_note: "PowerPoint で開いているファイルは上書きされません。",
    advanced_settings: "詳細設定",
    ui_language: "表示言語",
    high_res_background: "高解像度背景（より鮮明、ファイルサイズ大）",
    pad_to_16x9: "ページを 16:9 にパディング",
    page_range: "ページ範囲",
    page_range_hint: "すべて（例：1-3,5）",
    models_heading: "モデル",
    model_ready: "準備完了",
    model_missing: "未ダウンロード",
    gpu_ready_note: "GPU 版準備完了（文字消去を GPU で実行）",
    gpu_pending_note: "GPU 版未生成（初回変換時に自動生成、約 1 秒）",
    download_missing_models: "不足しているモデルをダウンロード",
    downloading: "ダウンロード中…",
    models_fallback_note: "LaMa がない場合は文字のストロークのみ消去します。SAM がない場合は単色背景のページのみオブジェクトを切り出します。",
    models_downloaded: "モデルのダウンロードが完了しました。",
    results_title: "処理結果",
    status_waiting: "待機中",
    status_preparing: "準備中",
    status_done: "完了",
    status_skipped: "スキップ",
    status_failed: "失敗",
    status_cancelled: "キャンセル済",
    open_file: "開く",
    reveal_in_finder: "Finder で表示",
    nothing_to_convert: "変換するファイルがありません。",
    cancel: "キャンセル",
    cancelling: "キャンセル中…（現在のページ完了後に停止）",
    speed_estimate_note: "11 ページのスライド：標準版は約 0.5–1 分、オブジェクト版は約 1–1.5 分。",
    variant_standard_label: "標準版",
    variant_objects_label: "オブジェクト版",
    variant_standard_suffix: "_編集可能",
    variant_objects_suffix: "_編集可能_オブジェクト",
    plan_convert: "変換",
    plan_skip_exists: "既に存在するためスキップ（再作成するには「既存のファイルを上書き」にチェック）",
    plan_skip_locked: "PowerPoint で開かれているためスキップ（先に閉じてください）",
    stage_layout: "テキスト認識中",
    stage_save: "ファイル書き出し中",
    activity_converting: "PDF を変換中",
    activity_downloading: "モデルをダウンロード中",
    model_lama_name: "LaMa モデル（文字消去・背景復元、約 199 MiB）",
    model_sam_encoder_name: "SAM encoder（オブジェクト切り出し、約 27 MiB）",
    model_sam_decoder_name: "SAM decoder（オブジェクト切り出し、約 16 MiB）",
};

const ES: Strings = Strings {
    heading: "PDF de NotebookLM → PowerPoint editable",
    first_run_title: "Primer uso: descargar modelos",
    download_models: "Descargar modelos",
    first_run_note: "También puedes convertir sin descargar: la versión estándar solo borra los trazos del texto (menor calidad); la de objetos solo procesa páginas con fondo plano.",
    sources_title: "1. PDF de origen",
    drop_to_add: "Suelta para añadir",
    drag_or_add_pdf: "Arrastra archivos PDF a la ventana o pulsa «Añadir PDF…»",
    only_unadded_pdfs: "Solo se aceptan archivos PDF que aún no se hayan añadido.",
    remove: "Quitar",
    add_pdf: "Añadir PDF…",
    clear_all: "Borrar todo",
    output_version_title: "2. Versión de salida",
    standard_recommended: "Estándar (recomendado)",
    standard_note: "El texto se convierte en cuadros de texto editables; las imágenes quedan en el fondo. El nombre termina en «_editable».",
    objects_experimental: "Objetos (experimental)",
    objects_note: "También recorta iconos e ilustraciones como imágenes movibles. Al mover un objeto queda el fondo reconstruido, que puede tener manchas oscuras. El nombre termina en «_editable_objetos».",
    generate_both: "Generar ambas",
    missing_sam_warning: "La versión con objetos requiere el modelo SAM. Actualmente falta, por lo que solo se extraerán objetos en páginas con fondo plano. Descarga primero los modelos.",
    output_location_title: "3. Ubicación de salida",
    same_folder_as_pdf: "Misma carpeta que el PDF",
    custom_folder: "Elegir carpeta",
    save_to: "Guardar en",
    downloads_folder: "Descargas",
    not_selected: "(sin seleccionar)",
    select_output_folder_first: "Selecciona primero una carpeta de salida.",
    change_folder: "Cambiar…",
    overwrite_existing: "Sobrescribir archivos existentes",
    powerpoint_open_note: "Los archivos abiertos en PowerPoint nunca se sobrescriben.",
    advanced_settings: "Ajustes avanzados",
    ui_language: "Idioma",
    high_res_background: "Fondo de alta resolución (más nítido, archivo más grande)",
    pad_to_16x9: "Rellenar páginas a 16:9",
    page_range: "Rango de páginas",
    page_range_hint: "Todas (ej.: 1-3,5)",
    models_heading: "Modelos",
    model_ready: "Listo",
    model_missing: "No descargado",
    gpu_ready_note: "Versión GPU lista (la eliminación de texto se ejecuta en la GPU)",
    gpu_pending_note: "Versión GPU aún no generada (se genera automáticamente en la primera conversión, ~1 s)",
    download_missing_models: "Descargar modelos faltantes",
    downloading: "Descargando…",
    models_fallback_note: "Sin LaMa solo se borran los trazos del texto; sin SAM la versión con objetos solo procesa páginas con fondo plano.",
    models_downloaded: "Modelos descargados correctamente.",
    results_title: "Resultados",
    status_waiting: "En espera",
    status_preparing: "Preparando",
    status_done: "Completado",
    status_skipped: "Omitido",
    status_failed: "Error",
    status_cancelled: "Cancelado",
    open_file: "Abrir",
    reveal_in_finder: "Mostrar en el Finder",
    nothing_to_convert: "No hay archivos para convertir.",
    cancel: "Cancelar",
    cancelling: "Cancelando… (se detendrá al terminar la página actual)",
    speed_estimate_note: "Presentación de 11 páginas: ~0,5–1 min estándar, ~1–1,5 min con objetos.",
    variant_standard_label: "Estándar",
    variant_objects_label: "Objetos",
    variant_standard_suffix: "_editable",
    variant_objects_suffix: "_editable_objetos",
    plan_convert: "Convertir",
    plan_skip_exists: "Ya existe, omitido (marca «Sobrescribir archivos existentes» para rehacer)",
    plan_skip_locked: "Abierto en PowerPoint, omitido (ciérralo primero)",
    stage_layout: "Reconociendo texto",
    stage_save: "Escribiendo archivo",
    activity_converting: "Convirtiendo PDF",
    activity_downloading: "Descargando modelos",
    model_lama_name: "Modelo LaMa (eliminación de texto, ~199 MiB)",
    model_sam_encoder_name: "SAM encoder (extracción de objetos, ~27 MiB)",
    model_sam_decoder_name: "SAM decoder (extracción de objetos, ~16 MiB)",
};

const FR: Strings = Strings {
    heading: "PDF NotebookLM → PowerPoint modifiable",
    first_run_title: "Premier lancement : télécharger les modèles",
    download_models: "Télécharger les modèles",
    first_run_note: "Vous pouvez convertir sans télécharger : la version standard efface uniquement les traits du texte (qualité moindre) ; la version objets ne traite que les fonds unis.",
    sources_title: "1. PDF source",
    drop_to_add: "Relâchez pour ajouter",
    drag_or_add_pdf: "Glissez des fichiers PDF dans la fenêtre ou cliquez sur « Ajouter des PDF… »",
    only_unadded_pdfs: "Seuls les fichiers PDF non encore ajoutés sont acceptés.",
    remove: "Retirer",
    add_pdf: "Ajouter des PDF…",
    clear_all: "Tout effacer",
    output_version_title: "2. Version de sortie",
    standard_recommended: "Standard (recommandé)",
    standard_note: "Le texte devient des zones de texte modifiables ; les images restent en arrière-plan. Le nom se termine par « _modifiable ».",
    objects_experimental: "Objets (expérimental)",
    objects_note: "Découpe aussi les icônes et illustrations en images déplaçables. Déplacer un objet révèle un fond reconstruit pouvant présenter des taches sombres. Le nom se termine par « _modifiable_objets ».",
    generate_both: "Générer les deux",
    missing_sam_warning: "La version avec objets nécessite le modèle SAM. Sans lui, seules les pages à fond uni sont traitées. Veuillez d’abord télécharger les modèles.",
    output_location_title: "3. Dossier de sortie",
    same_folder_as_pdf: "Même dossier que le PDF",
    custom_folder: "Choisir un dossier",
    save_to: "Enregistrer dans",
    downloads_folder: "Téléchargements",
    not_selected: "(non sélectionné)",
    select_output_folder_first: "Veuillez d’abord sélectionner un dossier de sortie.",
    change_folder: "Modifier…",
    overwrite_existing: "Écraser les fichiers existants",
    powerpoint_open_note: "Les fichiers ouverts dans PowerPoint ne sont jamais écrasés.",
    advanced_settings: "Paramètres avancés",
    ui_language: "Langue",
    high_res_background: "Arrière-plan haute résolution (plus net, fichier plus lourd)",
    pad_to_16x9: "Adapter les pages en 16:9",
    page_range: "Pages",
    page_range_hint: "Toutes (ex. : 1-3,5)",
    models_heading: "Modèles",
    model_ready: "Prêt",
    model_missing: "Non téléchargé",
    gpu_ready_note: "Version GPU prête (suppression du texte sur le GPU)",
    gpu_pending_note: "Version GPU non générée (créée automatiquement à la première conversion, ~1 s)",
    download_missing_models: "Télécharger les modèles manquants",
    downloading: "Téléchargement…",
    models_fallback_note: "Sans LaMa, seuls les traits du texte sont effacés ; sans SAM, la version objets ne traite que les pages à fond uni.",
    models_downloaded: "Modèles téléchargés avec succès.",
    results_title: "Résultats",
    status_waiting: "En attente",
    status_preparing: "Préparation",
    status_done: "Terminé",
    status_skipped: "Ignoré",
    status_failed: "Échec",
    status_cancelled: "Annulé",
    open_file: "Ouvrir",
    reveal_in_finder: "Afficher dans le Finder",
    nothing_to_convert: "Aucun fichier à convertir.",
    cancel: "Annuler",
    cancelling: "Annulation… (arrêt après la page en cours)",
    speed_estimate_note: "Présentation de 11 pages : ~0,5–1 min en standard, ~1–1,5 min avec objets.",
    variant_standard_label: "Standard",
    variant_objects_label: "Objets",
    variant_standard_suffix: "_modifiable",
    variant_objects_suffix: "_modifiable_objets",
    plan_convert: "Convertir",
    plan_skip_exists: "Existe déjà, ignoré (cochez « Écraser les fichiers existants » pour refaire)",
    plan_skip_locked: "Ouvert dans PowerPoint, ignoré (veuillez d’abord le fermer)",
    stage_layout: "Reconnaissance du texte",
    stage_save: "Écriture du fichier",
    activity_converting: "Conversion du PDF",
    activity_downloading: "Téléchargement des modèles",
    model_lama_name: "Modèle LaMa (suppression du texte, ~199 Mo)",
    model_sam_encoder_name: "SAM encoder (extraction d’objets, ~27 Mo)",
    model_sam_decoder_name: "SAM decoder (extraction d’objets, ~16 Mo)",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bcp47_and_posix_locale_tags() {
        assert_eq!(Language::from_tag("zh-TW"), Some(Language::ZhTw));
        assert_eq!(Language::from_tag("zh-Hant-TW"), Some(Language::ZhTw));
        assert_eq!(Language::from_tag("zh_HK.UTF-8"), Some(Language::ZhTw));
        assert_eq!(Language::from_tag("zh-CN"), Some(Language::ZhCn));
        assert_eq!(Language::from_tag("zh-Hans-JP"), Some(Language::ZhCn));
        assert_eq!(Language::from_tag("zh_SG.UTF-8"), Some(Language::ZhCn));
        assert_eq!(Language::from_tag("ja"), Some(Language::Ja));
        assert_eq!(Language::from_tag("ja-JP"), Some(Language::Ja));
        assert_eq!(Language::from_tag("ja_JP.UTF-8"), Some(Language::Ja));
        assert_eq!(Language::from_tag("en-US"), Some(Language::En));
        assert_eq!(Language::from_tag("en_GB.UTF-8"), Some(Language::En));
        assert_eq!(Language::from_tag("es-419"), Some(Language::Es));
        assert_eq!(Language::from_tag("fr_FR@euro"), Some(Language::Fr));
        assert_eq!(Language::from_tag("de-DE"), None);
        assert_eq!(Language::from_tag(""), None);
    }

    #[test]
    fn all_six_languages_roundtrip_by_code() {
        for &lang in Language::ALL {
            assert_eq!(Language::from_tag(lang.code()), Some(lang));
            assert!(!lang.display_name().is_empty());
            assert!(!lang.strings().heading.is_empty());
        }
    }
}
