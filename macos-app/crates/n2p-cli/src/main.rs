//! `n2p` —— notebooklm2ppt 的 macOS 命令列介面。
//!
//! 與 Windows 版的差異：不再模擬滑鼠／截圖，也不需要 PowerPoint 或微軟電腦管家。
//! 可編輯文字來自 macOS 內建的 Vision OCR（或外部的 MinerU JSON），
//! PPTX 由 `n2p-ooxml` 直接產生。

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use n2p_build::BackgroundResolution;
use n2p_image::InpaintMethod;
use n2p_pipeline::{ConvertOptions, LayoutSource};
use n2p_vision::VisionConfig;

/// notebooklm2ppt：把 PDF 轉成可編輯的 PowerPoint。
#[derive(Debug, Parser)]
#[command(name = "n2p", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 把 PDF 轉成可編輯的 PPTX
    Convert(ConvertArgs),
    /// 顯示 PDF 的頁數與頁面尺寸
    Info(InfoArgs),
    /// 列出可用的去水印方法
    Methods,
    /// 對單張影像跑 OCR，用於診斷版面結果
    Ocr(OcrArgs),
    /// 列出候選東亞字型與其安裝狀態
    Fonts,
    /// 管理可選的推論模型（LaMa 去字修補）
    Models(ModelsArgs),
}

#[derive(Debug, Args)]
struct ModelsArgs {
    #[command(subcommand)]
    action: ModelsAction,
}

#[derive(Debug, Subcommand)]
enum ModelsAction {
    /// 下載 LaMa 模型到快取目錄（約 199 MiB，只需一次）
    Fetch,
    /// 顯示模型狀態與路徑
    Status,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TextModeArg {
    /// 擦除式（預設）：先從背景擦掉原稿文字筆畫，再疊上文字框
    Erase,
    /// 覆蓋式：背景不動，文字框自帶底色蓋住原稿文字
    Cover,
    /// 疊加式：背景完全不動、文字全透明。視覺等於原稿，文字仍可選取編輯
    Overlay,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum EraseArg {
    /// 有 LaMa 模型就用模型重建（品質最好），沒有就退回 `stroke`
    #[default]
    Auto,
    /// 只擦文字筆畫：子彈點、光暈、圖示不是文字色，完全不會被動到
    Stroke,
    /// 用 LaMa 模型重建筆畫與其光暈底下的紋理（需先 `n2p models fetch`）
    Lama,
    /// 擦整個矩形並從四周插值：會連同區域內的圖示一起抹掉
    Inpaint,
    /// 填單一顏色：與 Python 版等價
    Fill,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LayoutArg {
    /// 自動：macOS 用 Vision，其他平台退回 image
    Auto,
    /// macOS Vision OCR
    Vision,
    /// 不使用版面資訊（每頁一張滿版圖片）
    Image,
    /// 使用 MinerU／Paddle 的 JSON
    Mineru,
}

#[derive(Debug, Args)]
struct ConvertArgs {
    /// 來源 PDF
    input: PathBuf,

    /// 輸出 PPTX（預設與 PDF 同名）
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// 點陣化 DPI
    #[arg(long, default_value_t = 150.0)]
    dpi: f64,

    /// 不去除 NotebookLM 水印
    #[arg(long)]
    no_inpaint: bool,

    /// 去水印方法
    #[arg(long, value_name = "ID", default_value = "background_smooth")]
    method: String,

    /// 把頁面調整成 16:9
    #[arg(long)]
    widescreen: bool,

    /// 版面來源
    #[arg(long, value_enum, default_value = "auto")]
    layout: LayoutArg,

    /// MinerU／Paddle JSON（`--layout mineru` 時必填）
    #[arg(long, value_name = "FILE")]
    mineru: Option<PathBuf>,

    /// Vision 辨識語言，逗號分隔
    #[arg(
        long,
        value_name = "LANGS",
        default_value = "zh-Hant,zh-Hans,ja-JP,en-US,es-ES,fr-FR"
    )]
    lang: String,

    /// 只處理指定頁，例如 `1-3,5`
    #[arg(long, value_name = "RANGE")]
    pages: Option<String>,

    /// 拉丁字型；`auto` 表示依內文自動挑選
    #[arg(long, default_value = "auto")]
    font: String,

    /// 東亞字型（CJK 字元實際使用的字型）；預設自動挑選
    #[arg(long, value_name = "NAME")]
    ea_font: Option<String>,

    /// 讓 PowerPoint 自動縮放文字以塞入框內
    #[arg(long)]
    autofit: bool,

    /// 把背景上的獨立物件切成可移動的圖片（實驗性）。平坦底色免模型；
    /// 攝影或漸層背景需先 `n2p models fetch` 取得 SAM 模型
    #[arg(long)]
    extract_objects: bool,

    /// 背景保留完整點陣化解析度（檔案較大但較清晰）
    #[arg(long)]
    high_res_background: bool,

    /// 背景擦除方式：`auto`（預設）有 LaMa 模型就用、否則只擦筆畫；
    /// `lama` 強制用模型；`stroke` 只擦筆畫；`inpaint` 擦整個矩形；`fill` 填單色
    #[arg(long, value_enum, default_value = "auto")]
    erase: EraseArg,

    /// 文字合成方式：`cover` 不動背景、文字框自帶底色；`erase` 先擦背景再疊文字
    #[arg(long, value_enum, default_value = "erase")]
    text_mode: TextModeArg,

    /// 保留中間產物（頁面 PNG）到此目錄
    #[arg(long, value_name = "DIR")]
    work_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct InfoArgs {
    /// 來源 PDF
    input: PathBuf,
}

#[derive(Debug, Args)]
struct OcrArgs {
    /// 影像檔（PNG／JPEG）
    image: PathBuf,

    /// 辨識語言，逗號分隔
    #[arg(
        long,
        value_name = "LANGS",
        default_value = "zh-Hant,zh-Hans,ja-JP,en-US,es-ES,fr-FR"
    )]
    lang: String,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("錯誤：{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Convert(args) => convert(args),
        Command::Info(args) => info(args),
        Command::Methods => {
            methods();
            Ok(())
        }
        Command::Ocr(args) => ocr(args),
        Command::Fonts => {
            fonts();
            Ok(())
        }
        Command::Models(args) => models(args),
    }
}

fn models(args: ModelsArgs) -> anyhow::Result<()> {
    use n2p_image::models::ModelState;
    let entries = n2p_image::models::manifest().map_err(|e| anyhow::anyhow!(e))?;
    match args.action {
        ModelsAction::Status => {
            for model in &entries {
                let name = model.name;
                match model.state() {
                    ModelState::Missing => println!("{name}：未下載（執行 `n2p models fetch`）"),
                    ModelState::Ready => println!("{name}：已就緒"),
                    ModelState::Partial { size } => println!(
                        "{name}：不完整（{size} / {} 位元組），請重新執行 `n2p models fetch`",
                        model.expected_size
                    ),
                }
                println!("  路徑：{}", model.path.display());
                println!("  來源：{}", model.url);
                match model.gpu_converted() {
                    Some(true) => println!(
                        "  GPU 版：已就緒（{}）",
                        n2p_image::gpu::converted_path(&model.path).display()
                    ),
                    Some(false) => println!("  GPU 版：尚未產生（第一次使用時自動轉換，約 1 秒）"),
                    None => {}
                }
            }
            Ok(())
        }
        ModelsAction::Fetch => {
            for model in &entries {
                if model.state() == ModelState::Ready {
                    println!("已存在，無需下載：{}", model.path.display());
                    continue;
                }
                println!("下載 {} → {}", model.name, model.path.display());
                model.fetch(true).map_err(|e| anyhow::anyhow!(e))?;
                println!("完成：{}", model.path.display());
            }
            // 轉換失敗不算下載失敗：第一次載入時會再試，不行就用 CPU
            if let Err(error) = n2p_image::models::prepare_gpu(&entries) {
                eprintln!("警告：GPU 版 LaMa 轉換失敗（{error}），仍可用 CPU");
            }
            Ok(())
        }
    }
}

fn convert(args: ConvertArgs) -> anyhow::Result<()> {
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| args.input.with_extension("pptx"));

    let method = InpaintMethod::try_parse(&args.method).ok_or_else(|| {
        anyhow::anyhow!("未知的去水印方法 `{}`，可用值見 `n2p methods`", args.method)
    })?;

    let layout = match args.layout {
        LayoutArg::Auto => LayoutSource::Auto,
        LayoutArg::Image => LayoutSource::None,
        LayoutArg::Vision => LayoutSource::Vision(vision_config(&args.lang)?),
        LayoutArg::Mineru => {
            let path = args
                .mineru
                .clone()
                .ok_or_else(|| anyhow::anyhow!("`--layout mineru` 需要 `--mineru <FILE>`"))?;
            LayoutSource::Mineru(path)
        }
    };

    let options = ConvertOptions {
        dpi: args.dpi,
        inpaint: !args.no_inpaint,
        inpaint_method: method,
        widescreen: args.widescreen,
        layout,
        build: n2p_build::BuildOptions {
            font_name: args.font.clone(),
            east_asian_font_name: args.ea_font.clone(),
            widescreen: args.widescreen,
            text_autofit: args.autofit,
            extract_objects: args.extract_objects,
            text_mode: match args.text_mode {
                TextModeArg::Erase => n2p_build::TextMode::Erase,
                TextModeArg::Cover => n2p_build::TextMode::Cover,
                TextModeArg::Overlay => n2p_build::TextMode::Overlay,
            },
            erase_mode: match args.erase {
                EraseArg::Auto => {
                    if n2p_image::lama::default_model_path()
                        .map(|p| p.exists())
                        .unwrap_or(false)
                    {
                        n2p_build::EraseMode::Lama
                    } else {
                        n2p_build::EraseMode::StrokeMask
                    }
                }
                EraseArg::Lama => n2p_build::EraseMode::Lama,
                EraseArg::Stroke => n2p_build::EraseMode::StrokeMask,
                EraseArg::Inpaint => n2p_build::EraseMode::Inpaint,
                EraseArg::Fill => n2p_build::EraseMode::Flat,
            },
            background_resolution: if args.high_res_background {
                BackgroundResolution::FullImage
            } else {
                BackgroundResolution::PageSize
            },
            ..Default::default()
        },
        pages: args
            .pages
            .as_deref()
            .map(n2p_pipeline::parse_page_range)
            .transpose()?,
        work_dir: args.work_dir.clone(),
    };

    println!("來源：{}", args.input.display());
    let report = n2p_pipeline::convert(&args.input, &output, &options)?;

    println!("輸出：{}", output.display());
    println!("  頁數        {}", report.pages);
    println!("  畫布        {:?}", report.slide_size);
    println!("  版面來源    {}", report.layout_name);
    println!("  文字框      {}", report.text_boxes);
    println!("  圖片        {}", report.pictures);
    println!("  去水印頁數  {}", report.watermarks_removed);
    println!("  檔案大小    {} bytes", report.output_bytes);
    Ok(())
}

fn info(args: InfoArgs) -> anyhow::Result<()> {
    let doc = n2p_pdf::PdfDoc::open(&args.input)?;
    println!("{}", args.input.display());
    println!("  頁數   {}", doc.page_count());
    for index in 0..doc.page_count() {
        let (width, height) = doc.page_size_pt(index)?;
        let (px_w, px_h) = doc.pixel_size(index, 150.0)?;
        println!(
            "  page {}  {width:.2} x {height:.2} pt（{:.1} x {:.1} in）→ {px_w} x {px_h} px @ 150 dpi",
            index + 1,
            width / 72.0,
            height / 72.0
        );
    }
    Ok(())
}

fn methods() {
    println!("可用去水印方法（`--method`）：");
    for method in InpaintMethod::all() {
        let note = if method.is_bit_exact_with_python() {
            "與 Python 版逐位元相同"
        } else {
            "替代演算法，視覺接近"
        };
        println!(
            "  {:<18} {} —— {}",
            method.id(),
            method.display_name(),
            note
        );
        println!("  {:<18} {}", "", method.description());
    }
}

fn fonts() {
    println!("候選東亞字型（依偏好排序，實際使用的是第一個已安裝者）：");
    let mut picked = false;
    for (family, installed) in n2p_build::fonts::availability() {
        let mark = if installed {
            "✓ 已安裝"
        } else {
            "· 未安裝"
        };
        let tag = if installed && !picked {
            picked = true;
            "  ← 將使用"
        } else {
            ""
        };
        println!("  {mark}  {family}{tag}");
    }
    if !picked {
        println!("\n  找不到任何候選字型；<a:ea> 不會寫入，PowerPoint 會用主題字型。");
    }
    println!("\n可用 --font / --ea-font 指定其他字型。");
}

fn ocr(args: OcrArgs) -> anyhow::Result<()> {
    let image = n2p_core::RgbImage::open(&args.image)?;
    let config = vision_config(&args.lang)?;
    println!(
        "{}：{} x {}",
        args.image.display(),
        image.width,
        image.height
    );

    let observations = n2p_vision::recognize(&image, &config)?;
    println!("辨識 {} 筆：", observations.len());
    for (index, observation) in observations.iter().enumerate() {
        let (x, y, w, h) = observation.bbox_norm;
        println!(
            "  [{index}] conf={:.2} bbox=({x:.3}, {y:.3}, {w:.3}, {h:.3}) {:?}",
            observation.confidence, observation.text
        );
    }
    Ok(())
}

fn vision_config(langs: &str) -> anyhow::Result<VisionConfig> {
    let languages: Vec<String> = langs
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    if languages.is_empty() {
        return Err(anyhow::anyhow!("至少需要一個辨識語言"));
    }
    Ok(VisionConfig {
        languages,
        ..Default::default()
    })
}
