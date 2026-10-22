//! 介面背後的邏輯：輸出命名、要不要跳過、選項對應。不碰 egui，方便測試。

use std::path::{Path, PathBuf};

use n2p_build::{BackgroundResolution, EraseMode};
use n2p_pipeline::ConvertOptions;

use crate::i18n::Language;

/// 使用者選的輸出版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Standard,
    Objects,
    Both,
}

impl OutputMode {
    pub fn variants(self) -> &'static [Variant] {
        match self {
            OutputMode::Standard => &[Variant::Standard],
            OutputMode::Objects => &[Variant::Objects],
            OutputMode::Both => &[Variant::Standard, Variant::Objects],
        }
    }

    pub fn needs_sam(self) -> bool {
        self.variants().contains(&Variant::Objects)
    }
}

/// 實際產生的一份簡報。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// 文字變成文字框，其餘留在背景圖。
    Standard,
    /// 另把圖示、插圖切成可移動的圖片。
    Objects,
}

impl Variant {
    pub fn label(self, lang: Language) -> &'static str {
        let s = lang.strings();
        match self {
            Variant::Standard => s.variant_standard_label,
            Variant::Objects => s.variant_objects_label,
        }
    }

    pub fn suffix(self, lang: Language) -> &'static str {
        let s = lang.strings();
        match self {
            Variant::Standard => s.variant_standard_suffix,
            Variant::Objects => s.variant_objects_suffix,
        }
    }
}

/// 輸出檔路徑：`<PDF 檔名><後綴>.pptx`，放在 `out_dir`（未指定時與 PDF 同資料夾）。
pub fn output_path(
    pdf: &Path,
    out_dir: Option<&Path>,
    variant: Variant,
    lang: Language,
) -> PathBuf {
    let stem = pdf
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    let dir = out_dir
        .map(Path::to_path_buf)
        .or_else(|| pdf.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    dir.join(format!("{stem}{}.pptx", variant.suffix(lang)))
}

/// 使用者的「下載項目」。沙盒裡 `HOME` 是容器目錄，容器內的 `Downloads` 會連到
/// 真正的 `~/Downloads`（需要 `files.downloads.read-write` 權限）。
pub fn downloads_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Downloads"))
}

/// 面板選回 `~/Downloads` 時拿到的是真實路徑，不是容器內的連結，比對前先解析連結。
pub fn is_downloads(dir: &Path) -> bool {
    let canonical = |p: &Path| std::fs::canonicalize(p).ok();
    downloads_dir().is_some_and(|d| canonical(&d).is_some() && canonical(&d) == canonical(dir))
}

/// PowerPoint 開啟檔案時在同目錄建立的鎖檔：`~$<檔名>`。
pub fn lock_file(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("~${name}"))
}

/// 寫出時先寫到這個暫存檔，完成後再改名，失敗時不會毀掉既有的檔案。
pub fn partial_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.n2p-partial"))
}

/// 一個待處理的工作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub pdf: PathBuf,
    pub out: PathBuf,
    pub variant: Variant,
}

/// 工作的處理決定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    Convert,
    /// 輸出檔已存在，且未勾選覆寫。
    SkipExists,
    /// 輸出檔正在 PowerPoint 裡開著（有 `~$` 鎖檔）。無論是否勾選覆寫都跳過。
    SkipLocked,
}

impl Plan {
    pub fn describe(self, lang: Language) -> &'static str {
        let s = lang.strings();
        match self {
            Plan::Convert => s.plan_convert,
            Plan::SkipExists => s.plan_skip_exists,
            Plan::SkipLocked => s.plan_skip_locked,
        }
    }
}

/// 為每個 PDF、每個版本決定要做什麼。
pub fn plan_jobs(
    pdfs: &[PathBuf],
    mode: OutputMode,
    out_dir: Option<&Path>,
    overwrite: bool,
    lang: Language,
) -> Vec<(Job, Plan)> {
    let mut jobs = Vec::new();
    for pdf in pdfs {
        for &variant in mode.variants() {
            let out = output_path(pdf, out_dir, variant, lang);
            let plan = if lock_file(&out).exists() {
                Plan::SkipLocked
            } else if out.exists() && !overwrite {
                Plan::SkipExists
            } else {
                Plan::Convert
            };
            jobs.push((
                Job {
                    pdf: pdf.clone(),
                    out,
                    variant,
                },
                plan,
            ));
        }
    }
    jobs
}

/// 進階設定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// 背景保留完整解析度。目前所有實測都在這個設定下完成，所以預設開。
    pub high_res_background: bool,
    /// 頁碼範圍，例如 `1-3,5`；空白表示全部。
    pub pages: String,
    pub widescreen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            high_res_background: true,
            pages: String::new(),
            widescreen: false,
        }
    }
}

/// 轉成 pipeline 的選項（對應命令列的預設值）。
pub fn convert_options(
    variant: Variant,
    settings: &Settings,
    lama_ready: bool,
) -> Result<ConvertOptions, String> {
    let pages = match settings.pages.trim() {
        "" => None,
        text => Some(n2p_pipeline::parse_page_range(text).map_err(|e| e.to_string())?),
    };
    let mut options = ConvertOptions {
        widescreen: settings.widescreen,
        pages,
        ..Default::default()
    };
    options.build.widescreen = settings.widescreen;
    options.build.extract_objects = variant == Variant::Objects;
    // 與命令列 `--erase auto` 相同：有 LaMa 就用，否則只擦筆畫
    options.build.erase_mode = if lama_ready {
        EraseMode::Lama
    } else {
        EraseMode::StrokeMask
    };
    options.build.background_resolution = if settings.high_res_background {
        BackgroundResolution::FullImage
    } else {
        BackgroundResolution::PageSize
    };
    Ok(options)
}

/// 只接受副檔名是 `.pdf`（不分大小寫）的檔案。
pub fn is_pdf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
}

/// 加入檔案：只收 PDF、略過重複。回傳實際加入的數量。
pub fn add_pdfs(list: &mut Vec<PathBuf>, incoming: impl IntoIterator<Item = PathBuf>) -> usize {
    let mut added = 0;
    for path in incoming {
        if is_pdf(&path) && !list.contains(&path) {
            list.push(path);
            added += 1;
        }
    }
    added
}

/// 單一工作內的進度（0.0–1.0）與階段說明。產生投影片最慢，所以佔最大比例。
pub fn progress_fraction(progress: n2p_pipeline::Progress, lang: Language) -> (f32, String) {
    use n2p_pipeline::Progress;
    let s = lang.strings();
    let ratio = |page: usize, total: usize| page as f32 / total.max(1) as f32;
    match progress {
        Progress::Render { page, total } => (
            0.05 * ratio(page, total),
            lang.stage_render(page + 1, total),
        ),
        Progress::Layout => (0.05, s.stage_layout.into()),
        Progress::Build { page, total } => (
            0.15 + 0.8 * ratio(page, total),
            lang.stage_build(page + 1, total),
        ),
        Progress::Save => (0.95, s.stage_save.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("n2p-gui-tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn output_names_follow_the_variant() {
        let pdf = Path::new("/書稿/CH4_用信任點亮學生.pdf");
        assert_eq!(
            output_path(pdf, None, Variant::Standard, Language::ZhTw),
            Path::new("/書稿/CH4_用信任點亮學生_可編輯.pptx")
        );
        assert_eq!(
            output_path(
                pdf,
                Some(Path::new("/輸出")),
                Variant::Objects,
                Language::ZhTw
            ),
            Path::new("/輸出/CH4_用信任點亮學生_可編輯_物件.pptx")
        );
    }

    #[test]
    fn output_names_follow_the_selected_language() {
        let pdf = Path::new("/slides/deck.pdf");
        assert_eq!(
            output_path(pdf, None, Variant::Standard, Language::Ja),
            Path::new("/slides/deck_編集可能.pptx")
        );
        assert_eq!(
            output_path(pdf, None, Variant::Objects, Language::Ja),
            Path::new("/slides/deck_編集可能_オブジェクト.pptx")
        );
        assert_eq!(
            output_path(pdf, None, Variant::Standard, Language::En),
            Path::new("/slides/deck_editable.pptx")
        );
        assert_eq!(
            output_path(pdf, None, Variant::Objects, Language::ZhCn),
            Path::new("/slides/deck_可编辑_对象.pptx")
        );
        assert_eq!(
            output_path(pdf, None, Variant::Objects, Language::Es),
            Path::new("/slides/deck_editable_objetos.pptx")
        );
        assert_eq!(
            output_path(pdf, None, Variant::Objects, Language::Fr),
            Path::new("/slides/deck_modifiable_objets.pptx")
        );
    }

    #[test]
    fn modes_expand_to_variants() {
        assert_eq!(OutputMode::Standard.variants(), &[Variant::Standard]);
        assert_eq!(OutputMode::Objects.variants(), &[Variant::Objects]);
        assert_eq!(
            OutputMode::Both.variants(),
            &[Variant::Standard, Variant::Objects]
        );
        assert!(!OutputMode::Standard.needs_sam());
        assert!(OutputMode::Objects.needs_sam());
        assert!(OutputMode::Both.needs_sam());
    }

    #[test]
    fn lock_and_partial_files_sit_next_to_the_output() {
        let out = Path::new("/a/簡報_可編輯.pptx");
        assert_eq!(lock_file(out), Path::new("/a/~$簡報_可編輯.pptx"));
        assert_eq!(
            partial_path(out),
            Path::new("/a/.簡報_可編輯.pptx.n2p-partial")
        );
    }

    #[test]
    fn existing_outputs_are_skipped_unless_overwrite_is_on() {
        let dir = temp_dir("plan-exists");
        let pdf = dir.join("講義.pdf");
        std::fs::write(dir.join("講義_可編輯.pptx"), b"x").unwrap();

        let plans = plan_jobs(
            std::slice::from_ref(&pdf),
            OutputMode::Both,
            None,
            false,
            Language::ZhTw,
        );
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].1, Plan::SkipExists);
        assert_eq!(plans[1].1, Plan::Convert);

        let plans = plan_jobs(
            std::slice::from_ref(&pdf),
            OutputMode::Standard,
            None,
            true,
            Language::ZhTw,
        );
        assert_eq!(plans[0].1, Plan::Convert);
    }

    #[test]
    fn files_open_in_powerpoint_are_never_overwritten() {
        let dir = temp_dir("plan-locked");
        let pdf = dir.join("講義.pdf");
        std::fs::write(dir.join("講義_可編輯.pptx"), b"x").unwrap();
        std::fs::write(dir.join("~$講義_可編輯.pptx"), b"lock").unwrap();

        let plans = plan_jobs(
            std::slice::from_ref(&pdf),
            OutputMode::Standard,
            None,
            true,
            Language::ZhTw,
        );
        assert_eq!(plans[0].1, Plan::SkipLocked);
    }

    #[test]
    fn options_follow_variant_and_settings() {
        let settings = Settings::default();
        let standard = convert_options(Variant::Standard, &settings, true).unwrap();
        assert!(!standard.build.extract_objects);
        assert_eq!(standard.build.erase_mode, EraseMode::Lama);
        assert_eq!(
            standard.build.background_resolution,
            BackgroundResolution::FullImage
        );
        assert_eq!(standard.pages, None);

        let objects = convert_options(
            Variant::Objects,
            &Settings {
                high_res_background: false,
                pages: "1-2, 5".into(),
                widescreen: true,
            },
            false,
        )
        .unwrap();
        assert!(objects.build.extract_objects);
        assert_eq!(objects.build.erase_mode, EraseMode::StrokeMask);
        assert_eq!(
            objects.build.background_resolution,
            BackgroundResolution::PageSize
        );
        assert_eq!(objects.pages, Some(vec![1, 2, 5]));
        assert!(objects.widescreen && objects.build.widescreen);
    }

    #[test]
    fn bad_page_ranges_are_reported() {
        let settings = Settings {
            pages: "3-1".into(),
            ..Default::default()
        };
        assert!(convert_options(Variant::Standard, &settings, true).is_err());
    }

    #[test]
    fn only_new_pdfs_are_added() {
        let mut list = vec![PathBuf::from("/a/x.pdf")];
        let added = add_pdfs(
            &mut list,
            [
                PathBuf::from("/a/x.pdf"),
                PathBuf::from("/a/y.PDF"),
                PathBuf::from("/a/z.pptx"),
            ],
        );
        assert_eq!(added, 1);
        assert_eq!(
            list,
            vec![PathBuf::from("/a/x.pdf"), PathBuf::from("/a/y.PDF")]
        );
    }

    #[test]
    fn progress_is_monotonic_across_stages() {
        use n2p_pipeline::Progress;
        let steps = [
            Progress::Render { page: 0, total: 2 },
            Progress::Render { page: 1, total: 2 },
            Progress::Layout,
            Progress::Build { page: 0, total: 2 },
            Progress::Build { page: 1, total: 2 },
            Progress::Save,
        ];
        let fractions: Vec<f32> = steps
            .iter()
            .map(|p| progress_fraction(*p, Language::ZhTw).0)
            .collect();
        assert!(fractions.windows(2).all(|w| w[0] <= w[1]), "{fractions:?}");
        assert!(fractions.iter().all(|f| (0.0..1.0).contains(f)));
    }
}
