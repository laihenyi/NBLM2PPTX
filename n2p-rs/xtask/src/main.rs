//! `xtask` —— 開發用工具。
//!
//! 兩個子命令，對應兩種不同的問題：
//!
//! | 子命令 | 後端 | 回答的問題 |
//! | :--- | :--- | :--- |
//! | `compat` | Microsoft PowerPoint（AppleScript） | PowerPoint 接受我們產生的 OOXML 嗎？內容讀回來對嗎？ |
//! | `visual-diff` | LibreOffice（`soffice`） | 渲染出來看起來一樣嗎？ |
//!
//! 兩者刻意分開，因為 **PowerPoint 的 AppleScript 沒有匯出 PDF 的命令**
//! （只有 `save as picture` 與主題存檔），所以像素比對只能靠 LibreOffice。

mod compare;
mod manifest;
mod powerpoint;
mod soffice;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{Args, Parser, Subcommand};
use compare::Thresholds;

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "n2p-rs 的開發用工具", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 檢查可用的外部後端
    Doctor,
    /// 用 PowerPoint 開檔並比對讀回的內容
    Compat(CompatArgs),
    /// 用 LibreOffice 做像素級的視覺回歸
    VisualDiff(VisualDiffArgs),
}

#[derive(Debug, Args)]
struct CompatArgs {
    /// 要比對的 pptx（可重複）。省略時會即時產生一份範例簡報。
    #[arg(long = "file")]
    files: Vec<PathBuf>,

    /// 掃描目錄下所有 pptx。
    #[arg(long)]
    dir: Option<PathBuf>,

    /// 基準清單（預設 `fixtures/compat/manifest.json`）。
    #[arg(long)]
    manifest: Option<PathBuf>,

    /// 不進行比對，只把目前結果寫成基準清單。
    #[arg(long)]
    record: bool,
}

#[derive(Debug, Args)]
struct VisualDiffArgs {
    /// 要比對的 pptx（可重複）。
    #[arg(long = "file")]
    files: Vec<PathBuf>,

    /// 掃描目錄下所有 pptx。
    #[arg(long)]
    dir: Option<PathBuf>,

    /// 參考圖目錄（預設 `fixtures/visual`）。
    #[arg(long)]
    reference: Option<PathBuf>,

    /// 把目前的渲染結果寫成參考圖。
    #[arg(long)]
    record: bool,

    #[arg(long, default_value_t = 100.0)]
    dpi: f64,

    /// 平均通道差上限。
    #[arg(long, default_value_t = 6.0)]
    max_mean_abs: f64,

    /// 顯著差異像素比例上限。
    #[arg(long, default_value_t = 0.10)]
    max_bad_ratio: f64,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(failed) if failed > 0 => {
            eprintln!("\n{failed} 項未通過");
            ExitCode::FAILURE
        }
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("錯誤：{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<usize> {
    match cli.command {
        Command::Doctor => doctor(),
        Command::Compat(args) => compat(args),
        Command::VisualDiff(args) => visual_diff(args),
    }
}

// ── doctor ──────────────────────────────────────────────────────────────────

fn doctor() -> Result<usize> {
    println!("外部後端：");

    match powerpoint::version() {
        Ok(version) => println!("  Microsoft PowerPoint  {version}  → compat 可用"),
        Err(error) => println!("  Microsoft PowerPoint  不可用（{error}）→ compat 停用"),
    }

    match soffice::find() {
        Some(path) => {
            let version = soffice::version(&path).unwrap_or_else(|_| "版本未知".to_owned());
            println!("  LibreOffice           {version}");
            println!("                        路徑 {}", path.display());
            println!("                        → visual-diff 可用");
        }
        None => {
            println!("  LibreOffice           找不到 soffice → visual-diff 停用");
            println!("                        安裝：brew install --cask libreoffice");
            println!("                        或設定 SOFFICE_PATH 指向既有安裝");
        }
    }

    println!("\n純 Rust 部分（不需外部程式）：cargo test");
    Ok(0)
}

// ── compat ──────────────────────────────────────────────────────────────────

fn compat(args: CompatArgs) -> Result<usize> {
    powerpoint::version().context("PowerPoint 不可用；請確認已安裝並允許自動化權限")?;

    // 沒指定檔案時走「不變量檢查」：即時產生一份簡報，斷言 PowerPoint 讀回來的
    // 結構符合預期。這樣不需要把產出的二進位檔放進版控，也不會因為跨機器
    // OCR 差異而誤報。
    let explicit = collect_files(&args.files, args.dir.as_deref())?;
    if explicit.is_empty() && !args.record {
        return compat_invariants();
    }

    let manifest_path = args
        .manifest
        .clone()
        .unwrap_or_else(default_compat_manifest);
    let mut recorded = manifest::Manifest::default();
    let mut failures = 0usize;

    for file in &explicit {
        let name = entry_name(file);
        print!("{name:<24} ");
        let facts = match powerpoint::inspect(file) {
            Ok(facts) => facts,
            Err(error) => {
                println!("✗ 開檔失敗：{error}");
                failures += 1;
                continue;
            }
        };
        let entry = manifest::Entry::from_facts(name.clone(), facts);
        if args.record {
            println!(
                "記錄（{} 頁、寬 {:.0} pt、{} 個形狀）",
                entry.slides,
                entry.width_pt,
                entry
                    .pages
                    .iter()
                    .map(|p| p.shape_names.len())
                    .sum::<usize>()
            );
            recorded.entries.push(entry);
            continue;
        }

        let expected = match manifest::Manifest::load(&manifest_path) {
            Ok(value) => value,
            Err(error) => {
                println!("✗ 讀不到基準清單：{error}");
                failures += 1;
                continue;
            }
        };
        match expected.entries.iter().find(|e| e.name == name) {
            Some(want) => match want.compare(&entry) {
                Ok(()) => println!("✓ {} 頁", entry.slides),
                Err(diff) => {
                    println!("✗ {diff}");
                    failures += 1;
                }
            },
            None => {
                println!("✗ 基準清單沒有 `{name}`（先跑 --record）");
                failures += 1;
            }
        }
    }

    if args.record {
        if failures > 0 {
            // 不寫入殘缺的基準：那會讓下一次比對把「讀不到」誤當成「基準是空的」。
            return Err(anyhow!("有 {failures} 個檔案讀取失敗，未寫入基準清單"));
        }
        recorded.save(&manifest_path)?;
        println!(
            "\n已寫入 {}（{} 筆）",
            manifest_path.display(),
            recorded.entries.len()
        );
    } else if failures == 0 {
        println!("\n全部通過：PowerPoint 接受這些檔案，內容與基準一致");
    }
    Ok(failures)
}

/// 即時產生簡報並檢查 PowerPoint 讀回的結構是否符合預期。
fn compat_invariants() -> Result<usize> {
    let pdf = repo_root().join("fixtures/floyd.pdf");
    println!("即時產生簡報：{}（Vision OCR）", pdf.display());

    let mut failures = 0usize;
    for (label, layout) in [
        ("image", n2p_pipeline::LayoutSource::None),
        ("vision", n2p_pipeline::LayoutSource::Auto),
    ] {
        let out = std::env::temp_dir().join(format!("n2p-xtask-{label}.pptx"));
        let options = n2p_pipeline::ConvertOptions {
            pages: Some(vec![1]),
            layout: layout.clone(),
            ..Default::default()
        };
        n2p_pipeline::convert(&pdf, &out, &options)
            .map_err(|e| anyhow!("產生簡報失敗（{label}）：{e}"))?;

        let facts = powerpoint::inspect(&out)
            .with_context(|| format!("PowerPoint 開不了我們產生的檔案（{label}）"))?;

        print!("{:<10} ", format!("[{label}]"));
        let mut problems = Vec::new();
        if facts.slides != 1 {
            problems.push(format!("頁數 {}，預期 1", facts.slides));
        }
        // 高度無法透過 macOS AppleScript 取得，只驗寬度與 slide size 列舉。
        if (facts.width_pt - 960.0).abs() > 0.5 {
            problems.push(format!("畫布寬 {:.1} pt，預期 960.0", facts.width_pt));
        }
        if facts.slide_size_kind.is_empty() {
            problems.push("讀不到 slide size".to_owned());
        }
        let page = facts.pages.first();
        let shape_count = page.map(|p| p.shape_names.len()).unwrap_or(0);
        let text_count = page.map(|p| p.texts.len()).unwrap_or(0);
        let unnamed = page
            .map(|p| p.shape_names.iter().filter(|n| n.trim().is_empty()).count())
            .unwrap_or(0);
        if unnamed > 0 {
            problems.push(format!("有 {unnamed} 個形狀沒有名稱"));
        }
        match layout {
            n2p_pipeline::LayoutSource::None => {
                if shape_count != 0 || text_count != 0 {
                    problems.push(format!("圖片模式不應有形狀（實際 {shape_count} 個）"));
                }
            }
            _ => {
                if text_count == 0 {
                    problems.push("Vision 模式應產生文字框".to_owned());
                }
                let bad_names: Vec<&String> = page
                    .map(|p| {
                        p.shape_names
                            .iter()
                            .filter(|n| !n.starts_with("Block_"))
                            .collect()
                    })
                    .unwrap_or_default();
                if !bad_names.is_empty() {
                    problems.push(format!("形狀命名不符 Block_*：{bad_names:?}"));
                }
            }
        }

        if problems.is_empty() {
            println!(
                "✓ PowerPoint 開啟成功（寬 {:.0} pt、{shape_count} 形狀、{text_count} 文字框、{}）",
                facts.width_pt, facts.slide_size_kind
            );
        } else {
            println!("✗ {}", problems.join("；"));
            failures += 1;
        }
    }

    if failures == 0 {
        println!("\n全部通過：PowerPoint 接受我們產生的 OOXML，結構符合預期");
    }
    Ok(failures)
}

// ── visual-diff ─────────────────────────────────────────────────────────────

fn visual_diff(args: VisualDiffArgs) -> Result<usize> {
    let binary = soffice::find().ok_or_else(|| {
        anyhow!(
            "找不到 soffice。安裝 LibreOffice（brew install --cask libreoffice）或設定 SOFFICE_PATH"
        )
    })?;
    let reference_dir = args.reference.clone().unwrap_or_else(default_visual_dir);
    std::fs::create_dir_all(&reference_dir)?;

    let files = collect_files(&args.files, args.dir.as_deref())?;
    if files.is_empty() {
        return Err(anyhow!("沒有可比對的 pptx；請用 --file 或 --dir 指定"));
    }

    let libreoffice_version = soffice::version(&binary).unwrap_or_else(|_| "unknown".to_owned());
    let mut recorded: Vec<(String, usize)> = Vec::new();

    let thresholds = Thresholds {
        max_mean_abs: args.max_mean_abs,
        max_bad_ratio: args.max_bad_ratio,
        ..Thresholds::default()
    };
    let work_dir = std::env::temp_dir().join("n2p-visual-diff");
    let mut failures = 0usize;

    for file in &files {
        let name = entry_name(file);
        let pages = soffice::pptx_to_images(&binary, file, &work_dir, args.dpi)
            .with_context(|| format!("轉檔失敗：{}", file.display()))?;
        println!("{name:<24} {} 頁", pages.len());

        let file_ref_dir = reference_dir.join(&name);
        if args.record {
            std::fs::create_dir_all(&file_ref_dir)?;
            for (index, page) in pages.iter().enumerate() {
                page.save(file_ref_dir.join(format!("page_{:04}.png", index + 1)))?;
            }
            println!("  已記錄參考圖到 {}", file_ref_dir.display());
            recorded.push((name.clone(), pages.len()));
            continue;
        }

        // 參考圖是在特定 LibreOffice 版本與 DPI 下產生的；不一致就沒得比。
        if let Some(warning) = reference_mismatch(&reference_dir, args.dpi, &libreoffice_version) {
            println!("  ⚠ {warning}");
        }

        if !file_ref_dir.exists() {
            println!("  ✗ 沒有參考圖（先跑 --record）");
            failures += 1;
            continue;
        }

        for (index, page) in pages.iter().enumerate() {
            let reference_path = file_ref_dir.join(format!("page_{:04}.png", index + 1));
            if !reference_path.exists() {
                println!("  ✗ 缺少 {}", reference_path.display());
                failures += 1;
                continue;
            }
            let expected = n2p_core::RgbImage::open(&reference_path)?;
            match compare::diff(page, &expected, thresholds.pixel_threshold) {
                Some(diff) if diff.is_within(&thresholds) => println!(
                    "  第 {} 頁 ✓ 平均差 {:.2}、顯著差異 {:.2}%",
                    index + 1,
                    diff.mean_abs,
                    diff.bad_ratio * 100.0
                ),
                Some(diff) => {
                    println!(
                        "  第 {} 頁 ✗ 平均差 {:.2}（上限 {:.2}）、顯著差異 {:.2}%（上限 {:.2}%）、最大差 {}",
                        index + 1,
                        diff.mean_abs,
                        thresholds.max_mean_abs,
                        diff.bad_ratio * 100.0,
                        thresholds.max_bad_ratio * 100.0,
                        diff.max_channel
                    );
                    failures += 1;
                }
                None => {
                    println!("  第 {} 頁 ✗ 尺寸不符", index + 1);
                    failures += 1;
                }
            }
        }
    }

    if args.record {
        let meta = serde_json::json!({
            "generator": "xtask visual-diff --record",
            "libreoffice": libreoffice_version,
            "dpi": args.dpi,
            "pixel_threshold": thresholds.pixel_threshold,
            "files": recorded.iter().map(|(n, p)| serde_json::json!({"name": n, "pages": p})).collect::<Vec<_>>(),
        });
        std::fs::write(
            reference_dir.join("meta.json"),
            format!("{}\n", serde_json::to_string_pretty(&meta)?),
        )?;
        println!(
            "\n已記錄 {} 個檔案、{} 頁到 {}",
            recorded.len(),
            recorded.iter().map(|(_, p)| p).sum::<usize>(),
            reference_dir.display()
        );
    } else if failures == 0 {
        println!("\n全部通過");
    }
    Ok(failures)
}

/// 參考圖的產生環境與現在不同時回傳警告。
///
/// 換 LibreOffice 版本或改變 DPI 都會讓像素比對失去意義，先講清楚比對失敗的原因。
fn reference_mismatch(reference_dir: &Path, dpi: f64, current_version: &str) -> Option<String> {
    let path = reference_dir.join("meta.json");
    let text = std::fs::read_to_string(path).ok()?;
    let meta: serde_json::Value = serde_json::from_str(&text).ok()?;
    let recorded_dpi = meta.get("dpi").and_then(serde_json::Value::as_f64)?;
    let recorded_version = meta
        .get("libreoffice")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if (recorded_dpi - dpi).abs() > f64::EPSILON {
        return Some(format!(
            "參考圖以 {recorded_dpi} dpi 產生，目前是 {dpi} dpi，比對結果不可信"
        ));
    }
    if !recorded_version.is_empty() && recorded_version != current_version {
        return Some(format!(
            "參考圖由 LibreOffice「{recorded_version}」產生，目前是「{current_version}」，字型與排版可能有差"
        ));
    }
    None
}

// ── 輔助 ────────────────────────────────────────────────────────────────────

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn default_compat_manifest() -> PathBuf {
    repo_root().join("fixtures/compat/manifest.json")
}

fn default_visual_dir() -> PathBuf {
    repo_root().join("fixtures/visual")
}

fn entry_name(file: &Path) -> String {
    file.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".to_owned())
}

fn collect_files(explicit: &[PathBuf], dir: Option<&Path>) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = explicit.to_vec();
    if let Some(dir) = dir {
        for entry in std::fs::read_dir(dir).with_context(|| format!("讀不到目錄 {dir:?}"))? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pptx"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}
