//! 版面模型 + 頁面影像 → PPTX。
//!
//! 移植自 `ppt_creater.py`（`create_ppt_from_paddle_json` 的主流程）與
//! `ppt_refiner.py` 的背景處理段落。
//!
//! ## 每一頁的組成
//!
//! ```text
//! 1. 建立一頁（從內嵌的空白範本複製，共用同一套 master/layout/theme）
//! 2. 文字區塊 → 文字框（依 bbox 高度推算字級、依 label 決定對齊與留白）
//! 3. 圖片/表格/圖表區塊 → 從頁面影像裁切 → 置底的圖片形狀
//! 4. 擦除已轉成文字框或圖片的區域 → 剩下的影像當投影片背景
//! ```
//!
//! 第 2 步先做、第 3 步置底，因此視覺層序是：背景 < 圖片 < 文字。
//!
//! ## 與 Python 版的刻意差異
//!
//! * **不需要 `clean_ppt`**：自己產生 XML，不會有 Spire 的評估水印。
//! * **背景解析度可選**：Python 版把頁面 PNG 縮到「點數尺寸」（例如 2867×1600
//!   縮成 1376×768）再當背景，等於丟掉一半解析度。這裡預設維持原行為以求等價，
//!   但可以用 [`BackgroundResolution::FullImage`] 保留完整解析度。

pub mod fonts;
pub mod fontsize;

use n2p_core::{BBox, RgbImage};
use n2p_image::{Mask, edge_diversity};
use n2p_layout::{Block, BlockKind, LayoutPage};
use n2p_ooxml::merge;
use n2p_ooxml::{Align, AutoFit, Deck, Package, Rect, SlideSize, TextBox};

/// 內嵌的空白簡報範本。
///
/// 由 Spire.Presentation 產生後移除所有形狀與評估水印，只留下 master/layout/theme。
/// 27 KB，含 11 個版面配置。
pub const BLANK_TEMPLATE: &[u8] = include_bytes!("../assets/blank.pptx");

/// 文字框的垂直位移（`delta_y = 2`）。
const TEXT_DELTA_Y_PT: f64 = 2.0;
/// 一般文字的垂直留白。
const V_PADDING_PT: f64 = 5.0;
/// 一般文字的水平留白（Python 版是 `h_padding + h_padding`，各 15）。
const H_PADDING_BODY_PT: f64 = 15.0;
/// 標題的水平留白。
const H_PADDING_TITLE_PT: f64 = 5.0;
/// 最小字級。
const MIN_FONT_SIZE_PT: f64 = 8.0;
/// PowerPoint 單行行距約為 1.2 em。
const LINE_PITCH_RATIO: f64 = 1.2;
/// 文字框排版用的行距倍率（略大於 1.2，含 CJK 字型的額外行高）。
const LINE_SPACING_RATIO: f64 = 1.3;
/// 額外留白（以行數計），吸收字型升降部的差異。
const TEXT_SLACK_LINES: f64 = 0.25;
/// 非中文簡報的預設拉丁字型。
const DEFAULT_LATIN_FONT: &str = "Calibri";
/// 字級上限，避免異常邊界框產生荒謬結果。
const MAX_FONT_SIZE_PT: f64 = 400.0;
/// 使用者沒有指定字型時，代表「自動挑選」。
pub const AUTO_FONT: &str = "auto";

/// 內文是否以中文（全形字）為主。
fn deck_is_cjk_dominant(inputs: &[PageInput<'_>]) -> bool {
    let mut full = 0usize;
    let mut total = 0usize;
    for input in inputs {
        for block in input.layout.text_blocks() {
            for c in block.text.chars().filter(|c| !c.is_whitespace()) {
                total += 1;
                if fontsize::is_full_width(c) {
                    full += 1;
                }
            }
        }
    }
    total > 0 && (full as f64 / total as f64) >= 0.5
}
/// 背景擦除時用的顏色容差（`tolerance=20`）。
const ERASE_TOLERANCE: u8 = 20;
/// 前景圖片向外擴張的像素數（`expand_px=2`）。
const FOREGROUND_EXPAND_PX: i64 = 2;

/// 錯誤型別。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("PPTX 操作失敗：{0}")]
    Ooxml(#[from] n2p_ooxml::Error),

    #[error("影像操作失敗：{0}")]
    Image(String),

    #[error("沒有可用的頁面")]
    NoPages,

    #[error("頁面尺寸不合法：{0}")]
    BadPageSize(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// 文字與背景的合成方式。
///
/// * [`TextMode::Erase`]：先從背景圖擦掉原稿文字，再把文字框疊上去。
///   需要「像素級正確」的擦除 —— 位置、子彈點、漸層、外框光暈都要對，
///   任何一點沒對上就是可見瑕疵。
/// * [`TextMode::Cover`]：**背景圖完全不動**，每個文字框自帶不透明底色矩形，
///   直接把原稿的文字（連同外框光暈）蓋掉。圖示、裝飾因此不可能被破壞。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextMode {
    /// 覆蓋式：背景不動，文字框自帶底色把原稿文字蓋掉。
    ///
    /// 實測**不比擦除式好**：覆蓋矩形必須比原稿文字大才蓋得住，於是又會壓到
    /// 旁邊的圖示；底色是單一顏色，遇到漸層就看得出來。保留為可選項。
    Cover,
    /// 擦除式（預設）：先擦背景再疊文字。
    #[default]
    Erase,
    /// 疊加式：**背景完全不動**，文字設為全透明。
    ///
    /// 視覺上等於原稿（逐像素相同），文字框仍可選取、可編輯 —— 改動它
    /// （或改顏色）就會顯示出來。這是「文字帶大片光暈、壓在照片上」這類
    /// 無法可靠擦除的頁面的正解（例如 CH7 的深色攝影背景簡報）。
    Overlay,
}

/// 背景擦除的方式。
///
/// Python 版是「把區塊填成邊緣主色」（[`EraseMode::Flat`]）：背景是純色時看不出來，
/// 但遇到漸層或跨越兩種顏色（例如文字壓在藍色圓角框上）就會留下明顯方塊。
///
/// [`EraseMode::Inpaint`] 改用 `n2p-image` 的修補引擎，從四周既有像素插值，
/// 對插圖型簡報明顯較好。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EraseMode {
    /// 填單一顏色。與 Python 版等價。
    Flat,
    /// 從四周插值。
    Inpaint,
    /// 只修補文字筆畫（預設）。子彈點、光暈、圖示因為不是文字色，
    /// 完全不會被動到 —— 不需要任何幾何啟發式。
    #[default]
    StrokeMask,
}

/// 背景圖的解析度策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackgroundResolution {
    /// 縮到頁面點數尺寸。與 Python 版等價，但會丟失解析度。
    #[default]
    PageSize,
    /// 保留原始點陣化解析度（檔案較大，但背景更清晰）。
    FullImage,
}

/// 產生選項。
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub font_name: String,
    /// 東亞字型。`None` 時依內文是否以中文為主自動選擇。
    pub east_asian_font_name: Option<String>,
    /// 是否把頁面調整成 16:9。
    pub widescreen: bool,
    /// 是否在背景圖上擦除已轉成文字/圖片的區域。
    pub erase_background_regions: bool,
    /// 擦除方式（僅 `TextMode::Erase` 時有效）。
    pub erase_mode: EraseMode,
    /// 文字與背景的合成方式。
    pub text_mode: TextMode,
    pub background_resolution: BackgroundResolution,
    /// 文字框是否寫入 `<a:normAutofit/>`。
    ///
    /// Python 版（經 Spire）不會寫，因此溢出時文字會超出框外；開啟可修好這件事。
    pub text_autofit: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            font_name: AUTO_FONT.to_owned(),
            east_asian_font_name: None,
            widescreen: false,
            erase_background_regions: true,
            erase_mode: EraseMode::default(),
            text_mode: TextMode::default(),
            background_resolution: BackgroundResolution::PageSize,
            text_autofit: false,
        }
    }
}

/// 一頁的輸入。
pub struct PageInput<'a> {
    pub layout: &'a LayoutPage,
    /// 已去水印（或未去水印）的頁面影像，像素座標。
    pub image: &'a RgbImage,
}

/// 單頁建置的統計，方便診斷「為什麼某頁沒有文字」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageReport {
    pub text_boxes: usize,
    pub skipped_text: usize,
    pub pictures: usize,
}

/// 整體建置結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub pages: Vec<PageReport>,
    pub slide_size: SlideSize,
}

impl Default for BuildReport {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            slide_size: SlideSize::Screen4x3,
        }
    }
}

/// 依版面與影像產生整份簡報。
pub fn build_deck(inputs: &[PageInput<'_>], options: &BuildOptions) -> Result<(Deck, BuildReport)> {
    if inputs.is_empty() {
        return Err(Error::NoPages);
    }

    // 空白範本：只留一頁，其餘頁面靠附加複製。
    let template = Deck::from_package(Package::from_bytes(BLANK_TEMPLATE).map_err(Error::Ooxml)?)
        .map_err(Error::Ooxml)?;
    let mut deck = template.clone();
    deck.keep_only_slide(0).map_err(Error::Ooxml)?;

    let first = &inputs[0];
    let slide_size = SlideSize::from_ratio(first.layout.size_pt.0, first.layout.size_pt.1);
    deck.set_slide_size(slide_size).map_err(Error::Ooxml)?;

    // CJK 字元用的是 `<a:ea>` 而不是 `<a:latin>`。不指定就會落到主題字型，
    // 字高與原稿差約一成。以內文是否以中文為主來決定預設值。
    //
    // 寫死字型名很危險：沒安裝時 PowerPoint 會靜默落到主題字型，等於沒設定。
    // 因此改成偵測系統實際裝了什麼，挑第一個可用的候選。
    let cjk = deck_is_cjk_dominant(inputs);
    let picked = cjk.then(fonts::pick_east_asian).flatten();
    let east_asian = options
        .east_asian_font_name
        .clone()
        .or_else(|| picked.map(str::to_owned));

    // 中文簡報若沒指定拉丁字型，也用同一個字型 —— Noto Sans CJK 含完整拉丁字形，
    // 整份簡報只用一個字型最接近原稿。
    let latin = if options.font_name == AUTO_FONT {
        picked.unwrap_or(DEFAULT_LATIN_FONT).to_owned()
    } else {
        options.font_name.clone()
    };

    let mut report = BuildReport {
        pages: Vec::with_capacity(inputs.len()),
        slide_size,
    };

    for (index, input) in inputs.iter().enumerate() {
        if index > 0 {
            merge::append_slide(&mut deck, &template, 0).map_err(Error::Ooxml)?;
        }
        let page_report = build_page(
            &mut deck,
            index,
            input,
            &slide_size,
            options,
            east_asian.as_deref(),
            &latin,
        )?;
        report.pages.push(page_report);
    }

    deck.sync_app_properties().map_err(Error::Ooxml)?;
    Ok((deck, report))
}

fn build_page(
    deck: &mut Deck,
    index: usize,
    input: &PageInput<'_>,
    slide_size: &SlideSize,
    options: &BuildOptions,
    east_asian_font: Option<&str>,
    latin_font: &str,
) -> Result<PageReport> {
    let layout = input.layout;
    let (slide_cx, slide_cy) = slide_size.emu();
    let slide_width_pt = slide_cx as f64 / 12_700.0;
    let slide_height_pt = slide_cy as f64 / 12_700.0;

    if layout.size_pt.0 <= 0.0 || layout.size_pt.1 <= 0.0 {
        return Err(Error::BadPageSize(format!("{:?}", layout.size_pt)));
    }
    let scale = slide_width_pt / layout.size_pt.0;

    let mut report = PageReport::default();
    let image_scale = input.image.width as f64 / layout.size_pt.0;

    // ── 1. 文字框 ──────────────────────────────────────────────────────────
    //
    // 同時記下「真的轉成文字框」的區塊與其擦除範圍。背景擦除只處理這些 ——
    // 沒轉成文字框的區塊（例如只有一個 `◎` 的子彈點）必須留在背景上，
    // 擦掉就等於把原稿的圖示刪了。
    let mut erase_regions: Vec<EraseRegion> = Vec::new();
    for block in layout.text_blocks() {
        // 先用幾何判斷把左側的項目符號／小圖示從 bbox 排除，再拿「純文字 bbox」
        // 去算字級、置中與擦除。少了這一步，bbox 含子彈點會讓字級偏大、
        // 文字置中後壓到子彈點上。
        let text_region = trim_block_icon(input.image, block.bbox, image_scale, &block.text);

        // 短小的非中文區塊若壓在圖形上，通常是 Vision 把裝飾讀成文字
        // （第 3 頁的引號裝飾被讀成 "99"）。這種區塊留給背景，不要重複畫一次。
        if is_decoration_block(input.image, text_region, image_scale, &block.text) {
            report.skipped_text += 1;
            continue;
        }
        // 原稿的文字顏色（未必是黑；CH9 是深藍）。
        // `None` = 文字色與背景太接近，無法可靠分離。
        let sampled_text_color = sample_text_color(input.image, text_region, image_scale);
        let text_color = sampled_text_color.unwrap_or([0, 0, 0]);
        // 先算遮罩：`None` 代表這塊壓在紋理（照片、木紋）上，
        // 遮罩不可靠 —— 這時改用「透明文字」（OCR overlay）：
        // 背景一個像素都不動，文字框存在但看不見，點進去改字才顯示。
        let em_px = estimate_font_size(block, scale) / scale.max(f64::EPSILON) * image_scale;
        let stroke_mask = sampled_text_color.and_then(|color| {
            build_stroke_mask(input.image, text_region, image_scale, color, em_px)
        });

        let env = TextEnv {
            scale,
            slide_width_pt,
            slide_height_pt,
            latin_font,
            east_asian_font,
            text_autofit: options.text_autofit,
            text_mode: options.text_mode,
        };
        let Some(text_box) = text_box_for(block, text_region, &env, text_color) else {
            report.skipped_text += 1;
            continue;
        };
        let text_box = if options.text_mode == TextMode::Overlay {
            // 疊加式：文字全透明，背景不動
            text_box.text_alpha(0)
        } else if stroke_mask.is_none() && options.text_mode == TextMode::Erase {
            // 紋理背景：文字透明，避免與原稿文字重疊
            text_box.text_alpha(0)
        } else if options.text_mode == TextMode::Cover {
            // 取文字框外圍的顏色當底色 —— 那一圈基本上是背景
            let pad_px =
                (text_box.font_size_pt / scale.max(f64::EPSILON) * image_scale * 0.35) as i64 + 3;
            let fill = background_color(input.image, text_region.scale(image_scale), pad_px);
            text_box.fill(fill)
        } else {
            text_box
        };
        deck.push_text_box(index, &text_box).map_err(Error::Ooxml)?;
        report.text_boxes += 1;

        // 擦除範圍 = 同一塊「純文字」區域，因此字級、位置、擦除三者一致。
        if options.text_mode == TextMode::Overlay {
            continue;
        }
        if let Some(mask) = stroke_mask {
            erase_regions.push(EraseRegion {
                region: text_region,
                mask,
            });
        }
    }

    // ── 2. 前景圖片（裁切後置底）───────────────────────────────────────────
    for block in layout.foreground_blocks() {
        let expanded = expand_bbox(block.bbox, FOREGROUND_EXPAND_PX)
            .clamp(input.image.width as i64, input.image.height as i64);
        if expanded.is_empty() {
            continue;
        }
        let crop = input.image.crop(expanded);
        let png = crop.to_png().map_err(|e| Error::Image(e.to_string()))?;
        let rect = ppt_rect(block.bbox, scale);
        deck.insert_picture_at_bottom(index, png, "png", rect, "fg")
            .map_err(Error::Ooxml)?;
        report.pictures += 1;
    }

    // ── 3. 背景 ────────────────────────────────────────────────────────────
    let background = prepare_background(input, options, &erase_regions)?;
    deck.set_background_picture(index, background, "png")
        .map_err(Error::Ooxml)?;

    let _ = image_scale;
    Ok(report)
}

/// 由區塊產生文字框；跳過空白與 NotebookLM 頁尾水印。
/// 產生文字框所需的環境資訊（避免參數列過長）。
struct TextEnv<'a> {
    scale: f64,
    slide_width_pt: f64,
    slide_height_pt: f64,
    latin_font: &'a str,
    east_asian_font: Option<&'a str>,
    text_autofit: bool,
    text_mode: TextMode,
}

fn text_box_for(
    block: &Block,
    region: BBox,
    env: &TextEnv<'_>,
    text_color: [u8; 3],
) -> Option<TextBox> {
    let TextEnv {
        scale,
        slide_width_pt,
        slide_height_pt,
        latin_font,
        east_asian_font,
        text_autofit,
        text_mode,
    } = *env;
    if should_skip(block) {
        return None;
    }

    let bx1 = region.left as f64;
    let by1 = region.top as f64;
    let bx2 = region.right as f64;
    let by2 = region.bottom as f64;

    let font_size_pt = estimate_font_size(block, scale);

    let is_title = block.kind == BlockKind::ParagraphTitle;
    let (alignment, h_padding) = if is_title {
        (Align::Left, H_PADDING_TITLE_PT)
    } else {
        (Align::Center, H_PADDING_BODY_PT)
    };

    let left = bx1 * scale;
    let top = by1 * scale + TEXT_DELTA_Y_PT;
    let right = bx2 * scale;
    let bottom = by2 * scale + TEXT_DELTA_Y_PT;

    let box_width_pt = right - left;
    let box_height_pt = bottom - top;
    if box_width_pt <= 0.0 || box_height_pt <= 0.0 {
        return None;
    }

    // 依「字級 × 行數」重算需要的空間。
    //
    // 不能直接用 bbox 當框：bbox 是貼齊墨跡的，而 PowerPoint 會在第一行上方加行距、
    // 最後一行下方加 descender，所以兩行字的墨跡框裝不下兩行字。字級估得越準，
    // 這個誤差越明顯（文字溢出框外，甚至被折行）。
    let line_count = block.lines.len().max(1) as f64;
    // 用剝掉項目符號後的文字算寬度 —— 否則會把子彈點的寬度算進去，
    // 文字框因此變寬、置中後往左壓到子彈點上。
    // 框寬用「最寬的那一行」，不是所有行的總和 ——
    // 舊版把整段的字數加起來，多行區塊的框會寬到蓋住旁邊的插圖。
    let units = block
        .lines
        .iter()
        .map(|line| fontsize::full_width_units(strip_leading_bullet(&line.text()).0))
        .fold(0.0_f64, f64::max)
        .max(fontsize::full_width_units(strip_leading_bullet(&block.text).0) / line_count);
    let needed_width_pt = (units * font_size_pt).max(box_width_pt);
    // 高度取「估算值」與「原 bbox」的較大者：字型的行距比原稿緊時，
    // 純估算會讓框比原稿矮，覆蓋式就會露出下面的原稿文字。
    let needed_height_pt =
        ((line_count * LINE_SPACING_RATIO + TEXT_SLACK_LINES) * font_size_pt).max(box_height_pt);
    // 覆蓋式要把原稿的外框光暈也蓋住，留白稍大一點
    let cover_padding_pt = if text_mode == TextMode::Cover {
        font_size_pt * 0.18
    } else {
        0.0
    };

    let rect = match alignment {
        // 內文（Vision 的全部文字）：以原 bbox 中心為中心放大
        Align::Center => {
            let cx = (left + right) / 2.0;
            let cy = (top + bottom) / 2.0;
            let half_w = (needed_width_pt / 2.0) + h_padding;
            let half_h = (needed_height_pt / 2.0) + V_PADDING_PT;
            Rect::ltrb(
                (cx - half_w).max(0.0),
                (cy - half_h).max(0.0),
                (cx + half_w).min(slide_width_pt),
                (cy + half_h).min(slide_height_pt),
            )
        }
        // 標題（MinerU 的 paragraph_title）：保留左緣
        _ => Rect::ltrb(
            left.max(0.0),
            (top - V_PADDING_PT).max(0.0),
            (left + needed_width_pt + h_padding * 2.0).min(slide_width_pt),
            (bottom + needed_height_pt - box_height_pt + V_PADDING_PT).min(slide_height_pt),
        ),
    };
    if rect.width_pt() <= 0.0 || rect.height_pt() <= 0.0 {
        return None;
    }
    // 覆蓋式要在文字周圍多留一點，把原稿的外框光暈一起蓋住
    let rect = Rect::ltrb(
        (rect.left_pt - cover_padding_pt).max(0.0),
        (rect.top_pt - cover_padding_pt).max(0.0),
        (rect.right_pt + cover_padding_pt).min(slide_width_pt),
        (rect.bottom_pt + cover_padding_pt).min(slide_height_pt),
    );

    let mut text_box = TextBox::new(
        format!("Block_{}", block.kind.label()),
        strip_leading_bullet(&block.text).0.to_owned(),
        rect,
    )
    .font(latin_font, font_size_pt)
    .color(text_color)
    .align(alignment)
    // 原稿每個視覺行就是一行：不讓 PowerPoint 重新折行，位置才不會跑掉。
    .wrap(n2p_ooxml::Wrap::None);
    if let Some(ea) = east_asian_font {
        text_box = text_box.east_asian_font(ea);
    }
    if text_autofit {
        text_box = text_box.autofit(AutoFit::Normal);
    }
    Some(text_box)
}

/// 建立「只含文字筆畫」的遮罩；回傳 `None` 代表這一塊無法可靠分離（見下）。
///
/// 判準是**顏色距離**：只有「接近取樣文字色」的像素才進遮罩。
/// 子彈點是黃底藍環、光暈是白色、裝飾是別的顏色，都不符條件，因此不會被動到。
///
/// 為什麼不用「模糊差異」（局部對比）判斷：bbox 是貼齊文字的，框內文字佔比很高，
/// 模糊後的平均值落在文字與背景之間，於是**文字與背景都會被判定為有差異**。
/// 實測 CH9 的正常文字區塊覆蓋率達 61–94%，完全不可用。
///
/// 回傳 `None` 的條件：取樣文字色與背景的對比太小（例如白字壓在淺色木紋上）。
/// 這種情況顏色距離無法分離筆畫，呼叫端應改用「透明文字」—— 不動背景、
/// 不產生雙重文字。
fn build_stroke_mask(
    image: &RgbImage,
    region: BBox,
    image_scale: f64,
    text_color: [u8; 3],
    em_px: f64,
) -> Option<Mask> {
    let bbox = region
        .scale(image_scale)
        .clamp(image.width as i64, image.height as i64);
    let mut mask = Mask::new(image.width, image.height);
    if bbox.is_empty() {
        return None;
    }

    let background = background_color(image, bbox, (bbox.height() / 4).max(6));
    let contrast = text_color
        .iter()
        .zip(background.iter())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    if contrast < 60 {
        if std::env::var("N2P_DEBUG_MASK").is_ok() {
            eprintln!("[mask] 對比太低（{contrast}）→ 改用透明文字 bbox={bbox:?}");
        }
        return None;
    }

    // 容差隨字級放大：大字的反鋸齒邊緣較寬
    let tolerance = (26.0 + em_px * 0.35).clamp(26.0, 80.0) as u8;
    let mut masked = 0usize;
    let total = (bbox.width() * bbox.height()).max(1) as usize;
    for y in bbox.top..bbox.bottom {
        for x in bbox.left..bbox.right {
            let pixel = image.pixel(x as u32, y as u32);
            let distance = pixel
                .iter()
                .zip(text_color.iter())
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(255);
            if distance <= tolerance {
                mask.set(x as u32, y as u32, true);
                masked += 1;
            }
        }
    }

    // 膨脹 1–3 px 吃掉反鋸齒邊緣。少了這步會留殘影，且背景 PNG 因高頻雜訊
    // 從 0.6 MB 漲到 1.6 MB/張。
    let grow = ((em_px * 0.06) as i64).clamp(1, 3);
    let original = mask.clone();
    for y in bbox.top..bbox.bottom {
        for x in bbox.left..bbox.right {
            if !original.get(x as u32, y as u32) {
                continue;
            }
            for dy in -grow..=grow {
                for dx in -grow..=grow {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx >= 0 && ny >= 0 && nx < image.width as i64 && ny < image.height as i64 {
                        mask.set(nx as u32, ny as u32, true);
                    }
                }
            }
        }
    }
    if std::env::var("N2P_DEBUG_MASK").is_ok() {
        eprintln!(
            "[mask] bbox={bbox:?} 對比={contrast} 遮罩 {:.0}%",
            masked as f64 / total as f64 * 100.0
        );
    }
    Some(mask)
}

/// 判斷是否為「裝飾被誤讀成文字」的區塊。
///
/// 條件：去符號後 ≤ 3 個字元、不含中日韓文字，而且所在區域色彩複雜
/// （代表它壓在圖形上，原稿的裝飾本來就會留在背景）。
fn is_decoration_block(image: &RgbImage, region: BBox, image_scale: f64, text: &str) -> bool {
    let display = strip_leading_bullet(text).0.trim();
    let count = display.chars().count();
    if count == 0 || count > 3 || display.chars().any(fontsize::is_full_width) {
        return false;
    }
    let bbox = region
        .scale(image_scale)
        .clamp(image.width as i64, image.height as i64);
    if bbox.is_empty() {
        return false;
    }
    let (diversity, _) = edge_diversity(image, bbox, ERASE_TOLERANCE);
    diversity > 0.35
}

/// 把文字區塊左側的項目符號／小圖示排除，回傳「純文字」的邊界框（layout pt）。
///
/// 兩階段：先用寬度粗估字級，拿它當「圖示最大寬度」去投影找空隙，再用縮小後的
/// 寬度重估字級。這樣字級、置中、擦除三者都對齊同一塊區域。
fn trim_block_icon(image: &RgbImage, bbox: BBox, image_scale: f64, text: &str) -> BBox {
    let units = fontsize::full_width_units(strip_leading_bullet(text).0);
    if units < 0.5 {
        return bbox;
    }
    let prelim_em = (bbox.width() as f64 / units).max(1.0);
    let trimmed = trim_leading_icon(image, bbox.scale(image_scale), prelim_em * image_scale);
    let trimmed = BBox::new(
        (trimmed.left as f64 / image_scale).round() as i64,
        (trimmed.top as f64 / image_scale).round() as i64,
        (trimmed.right as f64 / image_scale).round() as i64,
        (trimmed.bottom as f64 / image_scale).round() as i64,
    );
    if trimmed.width() >= 3 { trimmed } else { bbox }
}

/// 當作項目符號／裝飾的符號。
///
/// 這些是 Unicode 字元，Vision 會把它們讀成文字，於是同一個子彈點同時存在於
/// 背景圖（原稿的向量圖形）與文字框（我們新增的），看起來就是重疊錯位。
/// 全形標點（`「」`、`，`）**不在**此列 —— 那是內容的一部分。
const BULLET_CHARS: &[char] = &[
    '•', '‧', '∙', '◦', '○', '●', '◎', '◇', '◆', '■', '□', '▪', '▫', '▸', '‣', '⁃', '·', '※', '★',
    '☆', '✓', '✔', '→', '⇒', '➤', '❖', '❚',
];

/// 剝掉行首的項目符號，回傳 `(剩餘文字, 被剝掉的寬度（em）)`。
///
/// 只剝「行首連續的符號 + 空白」，內文的符號不動。
///
/// 回傳的 em 寬度用來決定**擦除要從哪裡開始** —— 原稿的子彈點是背景圖的一部分，
/// 擦掉就沒了，所以擦除範圍必須往右縮。
fn strip_leading_bullet(text: &str) -> (&str, f64) {
    let mut rest = text.trim_start();
    let mut advance = 0.0;
    while let Some(c) = rest.chars().next() {
        if BULLET_CHARS.contains(&c) {
            advance += if fontsize::is_full_width(c) { 1.0 } else { 0.5 };
            rest = &rest[c.len_utf8()..];
        } else if c.is_whitespace() {
            advance += 0.25;
            rest = &rest[c.len_utf8()..];
        } else {
            break;
        }
    }
    (rest, advance)
}

/// 對應 `should_skip_text_block`。
fn should_skip(block: &Block) -> bool {
    if !block.kind.is_text() {
        return true;
    }
    if strip_leading_bullet(&block.text).0.trim().is_empty() {
        // 純符號區塊（例如只有一個 `◎`）：不要產生文字框，讓原稿的圖示留在背景。
        return true;
    }
    // 跳過 NotebookLM 的頁尾水印文字
    if block.kind == BlockKind::Footer && block.text.to_lowercase().contains("notebooklm") {
        return true;
    }
    false
}

/// 估算區塊的字級（點）。
///
/// 三個訊號依可靠度排序，取加權結果：
///
/// 1. **行距**（多行時最穩）：與字形無關，PowerPoint 單行行距約 1.2 em。
/// 2. **字寬**（中文為主時最準）：CJK 等寬全形，一個字就是一個 em。
/// 3. **墨跡高度**（備援）：同一字級下 `一二三` 與 `提學國` 的墨跡差到 40%，
///    因此信心值最低。
///
/// 舊版用 `bbox高度 × 0.75`，對 Vision 的貼齊墨跡框會系統性偏小 30% 以上。
fn estimate_font_size(block: &Block, scale: f64) -> f64 {
    // 行距
    let by_pitch = line_pitch_pt(block)
        .map(|pitch| pitch * scale)
        .and_then(|pitch| fontsize::from_line_pitch(pitch, LINE_PITCH_RATIO));

    // 字寬 —— 用最寬的那一行，避免最後一行字少而被低估
    let widest = block
        .lines
        .iter()
        .filter_map(|line| line.bbox.map(|b| (line.text(), b.width() as f64 * scale)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let by_width = widest
        .as_ref()
        .and_then(|(text, width)| fontsize::from_width(text, *width));

    // 墨跡高度
    let ink_height = block.bbox.height().max(1) as f64 * scale / block.lines.len().max(1) as f64;
    let by_height = fontsize::from_ink_height(
        &block.text,
        ink_height,
        fontsize::ink_ratio_for(&block.text),
    );

    let em = if fontsize::full_width_ratio(&block.text) >= 0.5 {
        // 中文簡報：字寬幾乎是精確的，不要讓不可靠的墨跡高度把它拉走。
        by_width
            .map(|e| e.em_pt)
            .or_else(|| by_pitch.map(|e| e.em_pt))
            .or_else(|| by_height.map(|e| e.em_pt))
    } else {
        // 拉丁文字：字寬取決於字元組成、墨跡高度取決於有無升降部，兩者都不準。
        // 有行距就信行距，否則取兩者平均。
        by_pitch
            .map(|e| e.em_pt)
            .or_else(|| fontsize::blend(by_width, by_height))
    };

    let em = em
        .unwrap_or(MIN_FONT_SIZE_PT)
        .clamp(MIN_FONT_SIZE_PT, MAX_FONT_SIZE_PT)
        .round();
    if std::env::var("N2P_DEBUG_FONT").is_ok() {
        eprintln!(
            "[font] {:?} bbox={:?} em={em} units={:.2} lineWidth={:?} pitch={:?} height={:?} text={:?}",
            block.kind,
            block.bbox,
            fontsize::full_width_units(&block.text),
            widest.as_ref().map(|(_, w)| *w),
            by_pitch.map(|e| e.em_pt),
            by_height.map(|e| e.em_pt),
            block.text.chars().take(18).collect::<String>(),
        );
    }
    em
}

/// 相鄰行的垂直間距（取中位數，抗離群）。
fn line_pitch_pt(block: &Block) -> Option<f64> {
    let tops: Vec<f64> = block
        .lines
        .iter()
        .filter_map(|line| line.bbox.map(|b| b.top as f64))
        .collect();
    if tops.len() < 2 {
        return None;
    }
    let mut gaps: Vec<f64> = tops
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|g| *g > 0.5)
        .collect();
    if gaps.is_empty() {
        return None;
    }
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(gaps[gaps.len() / 2])
}

fn ppt_rect(bbox: BBox, scale: f64) -> Rect {
    Rect::ltrb(
        bbox.left as f64 * scale,
        bbox.top as f64 * scale,
        bbox.right as f64 * scale,
        bbox.bottom as f64 * scale,
    )
}

fn expand_bbox(bbox: BBox, expand: i64) -> BBox {
    BBox::new(
        bbox.left - expand,
        bbox.top - expand,
        bbox.right + expand,
        bbox.bottom + expand,
    )
}

/// 從原圖取樣文字顏色。
///
/// 兩個關鍵取捨：
///
/// 1. **背景要從 bbox 外圍取樣**。Vision 的框貼齊文字，密集段落裡文字面積大於
///    空白，直接用框內的眾數會把文字色當成背景。
/// 2. **文字色取「離背景最遠」的顏色，不是出現最多的顏色**。原稿常有白色外框／
///    光暈（CH9 的標題是深藍字 + 白光暈），光暈像素數往往多於字心，取眾數會得到
///    白色，產出「白字壓在淺底上」。
fn sample_text_color(image: &RgbImage, bbox: BBox, image_scale: f64) -> Option<[u8; 3]> {
    let bbox = bbox
        .scale(image_scale)
        .clamp(image.width as i64, image.height as i64);
    if bbox.is_empty() {
        return None;
    }
    let background = background_color(image, bbox, (bbox.height() / 3).max(6));
    let distance_from_bg = |c: [u8; 3]| -> u32 {
        c.iter()
            .zip(background.iter())
            .map(|(a, b)| a.abs_diff(*b) as u32)
            .max()
            .unwrap_or(0)
    };

    // 量化到 16 階分桶，記下每桶的像素數與總和
    let mut buckets: std::collections::HashMap<[u8; 3], (usize, [u64; 3])> =
        std::collections::HashMap::new();
    for y in bbox.top..bbox.bottom {
        for x in bbox.left..bbox.right {
            let pixel = image.pixel(x as u32, y as u32);
            if distance_from_bg(pixel) > 60 {
                let key = [pixel[0] & 0xF0, pixel[1] & 0xF0, pixel[2] & 0xF0];
                let entry = buckets.entry(key).or_insert((0, [0; 3]));
                entry.0 += 1;
                entry.1[0] += pixel[0] as u64;
                entry.1[1] += pixel[1] as u64;
                entry.1[2] += pixel[2] as u64;
            }
        }
    }
    let max_count = buckets.values().map(|(count, _)| *count).max()?;

    // 只考慮出現次數不算少的桶，避免單一雜訊像素勝出
    let threshold = (max_count / 10).max(4);
    let mut best: Option<(u32, [u8; 3])> = None;
    for (count, sums) in buckets.values() {
        if *count < threshold {
            continue;
        }
        let color = [
            (sums[0] / *count as u64) as u8,
            (sums[1] / *count as u64) as u8,
            (sums[2] / *count as u64) as u8,
        ];
        let distance = distance_from_bg(color);
        if best.is_none_or(|(d, _)| distance > d) {
            best = Some((distance, color));
        }
    }

    // 與背景差異太小代表「文字色與背景幾乎相同」（例如白字壓在淺色木紋上），
    // 這時顏色距離無法分離筆畫 —— 回傳 None，由呼叫端改用透明文字。
    let chosen = match best {
        Some((distance, color)) if distance >= 60 => Some(color),
        _ => None,
    };
    if std::env::var("N2P_DEBUG_MASK").is_ok() {
        eprintln!(
            "[color] bbox={:?} bg={:?} max_count={} chosen={:?}",
            bbox, background, max_count, chosen
        );
    }
    chosen
}

/// 估計文字區塊的背景色。
///
/// **不能直接用 bbox 內的最常見顏色**：Vision 的框貼齊文字，密集段落裡文字的
/// 面積往往大於空白，主色會變成文字色，後續「與背景不同 = 墨跡」的判斷就整個
/// 反過來。改成從 bbox 往外擴 `pad` 之後再取樣 —— 那一圈基本上是背景。
fn background_color(image: &RgbImage, bbox: BBox, pad: i64) -> [u8; 3] {
    let outer = BBox::new(
        bbox.left - pad,
        bbox.top - pad,
        bbox.right + pad,
        bbox.bottom + pad,
    )
    .clamp(image.width as i64, image.height as i64);
    if outer.is_empty() {
        return [255, 255, 255];
    }
    // 只取「外框減去 bbox」的那一圈。直接用整個外擴矩形不行：文字密集的區塊
    // 即使外擴了，框內仍是文字佔多數，主色會變成文字色。
    let mut counts: std::collections::HashMap<[u8; 3], usize> = std::collections::HashMap::new();
    for y in outer.top..outer.bottom {
        for x in outer.left..outer.right {
            let inside = x >= bbox.left && x < bbox.right && y >= bbox.top && y < bbox.bottom;
            if inside {
                continue;
            }
            *counts.entry(image.pixel(x as u32, y as u32)).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(c, _)| c)
        .unwrap_or([255, 255, 255])
}

/// 若區域左側是一個「孤立的窄圖形」（項目符號、小圖示），把它排除在擦除範圍外。
///
/// 判斷依據是**墨跡的水平投影**：原稿的子彈點與文字之間有明顯空隙，而文字本身
/// 字與字相連。所以「最左側的墨跡島寬度 ≤ 1.3 em，且後面接一個 ≥ 0.25 em 的空隙」
/// 就視為圖示，把擦除的左緣移到空隙之後。
///
/// 這比「靠 OCR 有沒有讀到符號」可靠：Vision 有時把 `◎` 讀成文字，有時完全不提，
/// 但幾何上空隙一定在。
fn trim_leading_icon(image: &RgbImage, bbox: BBox, em_px: f64) -> BBox {
    let bbox = bbox.clamp(image.width as i64, image.height as i64);
    if bbox.width() < 3 || bbox.height() < 3 || em_px <= 0.0 || !em_px.is_finite() {
        return bbox;
    }
    let background = background_color(image, bbox, (em_px * 0.5) as i64);
    // 每欄是否有「與背景明顯不同」的像素
    let mut ink = vec![false; bbox.width() as usize];
    for x in bbox.left..bbox.right {
        let mut has_ink = false;
        for y in bbox.top..bbox.bottom {
            let pixel = image.pixel(x as u32, y as u32);
            let distance = pixel
                .iter()
                .zip(background.iter())
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            if distance > 40 {
                has_ink = true;
                break;
            }
        }
        ink[(x - bbox.left) as usize] = has_ink;
    }

    // 最左側的連續墨跡島
    let Some(first) = ink.iter().position(|v| *v) else {
        return bbox;
    };
    let island_end = ink[first..]
        .iter()
        .position(|v| !*v)
        .map(|p| first + p)
        .unwrap_or(ink.len());
    let island_width = (island_end - first) as f64;
    // 這個簡報的子彈點是約 1.5 em 的圓形圖形，不是窄符號，門檻要放寬。
    if island_width > em_px * 2.5 {
        return bbox; // 太寬，是一整段文字
    }

    // 之後必須有夠寬的空隙才判定為圖示
    let gap_end = ink[island_end..]
        .iter()
        .position(|v| *v)
        .map(|p| island_end + p)
        .unwrap_or(ink.len());
    let gap_width = (gap_end - island_end) as f64;
    // 空隙要夠寬（文字的字距不會這麼大），而且後面要接一大段墨跡。
    if gap_width < em_px * 0.4 || gap_end >= ink.len() {
        return bbox;
    }
    let rest_ink = ink[gap_end..].iter().filter(|v| **v).count() as f64;
    if rest_ink < em_px * 2.0 || rest_ink <= island_width {
        return bbox;
    }

    if std::env::var("N2P_DEBUG_TRIM").is_ok() {
        eprintln!(
            "[trim] bbox={:?} em_px={:.1} island={:.0} gap={:.0} gap_end={} bg={:?}",
            bbox, em_px, island_width, gap_width, gap_end, background
        );
    }
    BBox::new(
        bbox.left + gap_end as i64,
        bbox.top,
        bbox.right,
        bbox.bottom,
    )
}

/// 準備背景影像：擦除「已轉成文字框」的區域。
///
/// 只擦 `erase_regions`（呼叫端算好的文字框範圍），不是整份版面的可擦除區塊。
/// 差別在於：沒有轉成文字框的區塊（例如被判定為純符號的子彈點）會原樣留在
/// 背景上，等於保留原稿的圖示。
/// 一個要從背景移除的區域。
#[derive(Debug, Clone)]
struct EraseRegion {
    region: BBox,
    /// 已算好的筆畫遮罩（只含文字筆畫）。
    mask: Mask,
}

fn prepare_background(
    input: &PageInput<'_>,
    options: &BuildOptions,
    erase_regions: &[EraseRegion],
) -> Result<Vec<u8>> {
    let _ = input.layout;
    let layout = input.layout;
    let mut image = match options.background_resolution {
        BackgroundResolution::FullImage => input.image.clone(),
        BackgroundResolution::PageSize => resize_to(
            input.image,
            layout.size_pt.0.round().max(1.0) as u32,
            layout.size_pt.1.round().max(1.0) as u32,
        )?,
    };

    if options.erase_background_regions && options.text_mode == TextMode::Erase {
        let image_scale = image.width as f64 / layout.size_pt.0;
        for EraseRegion { region, mask } in erase_regions {
            let bbox = region
                .scale(image_scale)
                .clamp(image.width as i64, image.height as i64);
            if bbox.is_empty() {
                continue;
            }
            match options.erase_mode {
                EraseMode::Flat => {
                    // Python 版行為：整塊填成邊緣主色。
                    let (_, fill_color) = edge_diversity(&image, bbox, ERASE_TOLERANCE);
                    for y in bbox.top..bbox.bottom {
                        for x in bbox.left..bbox.right {
                            image.set_pixel(x as u32, y as u32, fill_color);
                        }
                    }
                }
                EraseMode::Inpaint => {
                    // 從四周插值，避免在漸層或雙色背景上留下方塊。
                    n2p_image::inpaint_bbox_in_place(
                        &mut image,
                        bbox,
                        n2p_image::InpaintMethod::BackgroundSmooth,
                    );
                }
                EraseMode::StrokeMask => {
                    // 遮罩在文字階段就算好了，這裡只負責修補文字筆畫。
                    n2p_image::inpaint_mask_in_place(
                        &mut image,
                        mask,
                        n2p_image::InpaintMethod::BackgroundSmooth,
                    );
                }
            }
        }
    }

    image.to_png().map_err(|e| Error::Image(e.to_string()))
}

/// 用 Lanczos3 縮放（對應 Pillow 的 `Image.LANCZOS`）。
fn resize_to(image: &RgbImage, width: u32, height: u32) -> Result<RgbImage> {
    if image.width == width && image.height == height {
        return Ok(image.clone());
    }
    let buffer = image::RgbImage::from_raw(image.width, image.height, image.data.clone())
        .ok_or_else(|| Error::Image("無法建立影像緩衝".into()))?;
    let resized = image::imageops::resize(
        &buffer,
        width,
        height,
        image::imageops::FilterType::Lanczos3,
    );
    RgbImage::from_raw(width, height, resized.into_raw()).map_err(|e| Error::Image(e.to_string()))
}

/// 便利函式：把一組頁面直接輸出成單一 PPTX 檔。
pub fn build_to_file(
    inputs: &[PageInput<'_>],
    out: impl AsRef<std::path::Path>,
    options: &BuildOptions,
) -> Result<BuildReport> {
    let (deck, report) = build_deck(inputs, options)?;
    deck.save(out).map_err(Error::Ooxml)?;
    Ok(report)
}

/// 讀取內嵌範本（供測試與診斷）。
pub fn blank_template() -> Result<Deck> {
    Deck::from_package(Package::from_bytes(BLANK_TEMPLATE).map_err(Error::Ooxml)?)
        .map_err(Error::Ooxml)
}

#[cfg(test)]
mod tests {
    use super::*;
    use n2p_layout::{Line, Span};

    fn page() -> LayoutPage {
        LayoutPage {
            size_pt: (960.0, 540.0),
            blocks: vec![
                Block {
                    kind: BlockKind::ParagraphTitle,
                    bbox: BBox::new(60, 30, 660, 80),
                    text: "標題".into(),
                    lines: vec![Line {
                        spans: vec![Span {
                            text: Some("標題".into()),
                            image_path: None,
                        }],
                        bbox: None,
                    }],
                },
                Block {
                    kind: BlockKind::Text,
                    bbox: BBox::new(60, 200, 660, 260),
                    text: "內文".into(),
                    lines: vec![Line {
                        spans: vec![Span {
                            text: Some("內文".into()),
                            image_path: None,
                        }],
                        bbox: None,
                    }],
                },
            ],
        }
    }

    #[test]
    fn font_size_uses_line_width_for_chinese() {
        // 8 個全形字、框寬 336 pt → em 應約 42 pt，而不是舊公式的 336*0.75=252
        let mut block = Block::new(BlockKind::Text, BBox::new(0, 0, 336, 50));
        block.text = "提升學生正向幸福".into();
        block.lines = vec![n2p_layout::Line {
            spans: vec![n2p_layout::Span {
                text: Some(block.text.clone()),
                image_path: None,
            }],
            bbox: Some(BBox::new(0, 0, 336, 50)),
        }];
        let size = estimate_font_size(&block, 1.0);
        assert!((size - 42.0).abs() <= 1.0, "應約 42 pt，實際 {size}");
    }

    #[test]
    fn font_size_uses_line_pitch_for_multiline() {
        // 三行、行距 60 pt → em 應約 50 pt（60 / 1.2）
        let mut block = Block::new(BlockKind::Text, BBox::new(0, 0, 400, 160));
        block.text = "第一行\n第二行\n第三行".into();
        block.lines = (0..3)
            .map(|i| n2p_layout::Line {
                spans: vec![n2p_layout::Span {
                    text: Some(["第一行", "第二行", "第三行"][i].to_string()),
                    image_path: None,
                }],
                bbox: Some(BBox::new(0, i as i64 * 60, 150, i as i64 * 60 + 50)),
            })
            .collect();
        let size = estimate_font_size(&block, 1.0);
        assert!((size - 50.0).abs() <= 2.0, "應約 50 pt，實際 {size}");
    }

    #[test]
    fn font_size_is_clamped_to_sane_range() {
        let mut block = Block::new(BlockKind::Text, BBox::new(0, 0, 2, 2));
        block.text = "x".into();
        block.lines = vec![n2p_layout::Line {
            spans: vec![n2p_layout::Span {
                text: Some("x".into()),
                image_path: None,
            }],
            bbox: Some(BBox::new(0, 0, 2, 2)),
        }];
        let size = estimate_font_size(&block, 1.0);
        assert!(size >= MIN_FONT_SIZE_PT, "實際 {size}");
        assert!(size <= MAX_FONT_SIZE_PT);
    }

    #[test]
    fn footer_watermark_is_skipped() {
        let mut block = Block::new(BlockKind::Footer, BBox::new(0, 0, 10, 10));
        block.text = "Made with NotebookLM".into();
        assert!(should_skip(&block));
        block.text = "第 1 頁".into();
        assert!(!should_skip(&block));
    }

    #[test]
    fn empty_text_is_skipped() {
        let block = Block::new(BlockKind::Text, BBox::new(0, 0, 10, 10));
        assert!(should_skip(&block));
    }

    #[test]
    fn blank_template_is_usable() {
        let deck = blank_template().unwrap();
        assert_eq!(deck.slide_count().unwrap(), 1);
        let names = n2p_ooxml::slide::shape_names(&deck.slide(0).unwrap()).unwrap();
        assert!(names.is_empty(), "範本不應含任何形狀，實際 {names:?}");
    }

    #[test]
    fn builds_one_slide_per_page_with_text_and_background() {
        let layout = page();
        let image = RgbImage::filled(1920, 1080, [240, 240, 240]);
        let inputs = vec![
            PageInput {
                layout: &layout,
                image: &image,
            },
            PageInput {
                layout: &layout,
                image: &image,
            },
        ];
        let (deck, report) = build_deck(&inputs, &BuildOptions::default()).unwrap();

        assert_eq!(deck.slide_count().unwrap(), 2);
        assert_eq!(report.pages.len(), 2);
        assert_eq!(report.pages[0].text_boxes, 2);
        assert_eq!(report.slide_size, SlideSize::Screen16x9);

        for index in 0..2 {
            let doc = deck.slide(index).unwrap();
            assert!(
                n2p_ooxml::slide::background(&doc).is_some(),
                "第 {index} 頁應有背景"
            );
            let names = n2p_ooxml::slide::shape_names(&doc).unwrap();
            assert_eq!(
                names,
                vec![
                    "Block_paragraph_title".to_string(),
                    "Block_text".to_string()
                ]
            );
            assert!(n2p_ooxml::slide::background_blip_rid(&doc).is_some());
        }
    }

    #[test]
    fn foreground_blocks_become_pictures_below_text() {
        let layout = LayoutPage {
            size_pt: (960.0, 540.0),
            blocks: vec![
                Block::new(BlockKind::Text, BBox::new(10, 10, 100, 40)),
                Block::new(BlockKind::Image, BBox::new(200, 200, 400, 400)),
            ],
        };
        let image = RgbImage::filled(1920, 1080, [120, 130, 140]);
        let (deck, report) = build_deck(
            &[PageInput {
                layout: &layout,
                image: &image,
            }],
            &BuildOptions::default(),
        )
        .unwrap();

        assert_eq!(report.pages[0].pictures, 1);
        let doc = deck.slide(0).unwrap();
        let kinds = n2p_ooxml::slide::shape_kinds(&doc).unwrap();
        assert_eq!(
            kinds.first(),
            Some(&"p:pic"),
            "圖片應在最底層，實際 {kinds:?}"
        );
    }

    #[test]
    fn inpaint_erase_preserves_coloured_background_better_than_flat_fill() {
        // 重現第 1 頁的白斑：文字只有一部分壓在藍色方塊上，其餘在白色背景上。
        // 邊緣像素以白色居多 → 純色填色會整塊填白，把藍色方塊挖出一個白洞。
        let mut image = RgbImage::filled(400, 200, [255, 255, 255]);
        let blue = [80u8, 150, 220];
        for y in 60..140 {
            for x in 100..200 {
                image.set_pixel(x, y, blue);
            }
        }
        let text_bbox = BBox::new(150, 80, 300, 120);
        // 黑色「文字」只出現在藍色方塊內的那一段
        for y in 85..115 {
            for x in 155..195 {
                image.set_pixel(x, y, [0, 0, 0]);
            }
        }

        let mut block = Block::new(BlockKind::Text, text_bbox);
        block.text = "文字文字文字".into();
        block.lines = vec![n2p_layout::Line {
            spans: vec![n2p_layout::Span {
                text: Some(block.text.clone()),
                image_path: None,
            }],
            bbox: Some(text_bbox),
        }];
        let layout = LayoutPage {
            size_pt: (400.0, 200.0),
            blocks: vec![block],
        };
        let base = BuildOptions {
            background_resolution: BackgroundResolution::FullImage,
            text_mode: TextMode::Erase,
            ..Default::default()
        };

        let background_of = |mode: EraseMode| -> RgbImage {
            let options = BuildOptions {
                erase_mode: mode,
                ..base.clone()
            };
            let (deck, _) = build_deck(
                &[PageInput {
                    layout: &layout,
                    image: &image,
                }],
                &options,
            )
            .unwrap();
            let media = deck
                .package()
                .part_names()
                .find(|p| p.starts_with("ppt/media/"))
                .expect("應有背景影像")
                .to_owned();
            RgbImage::from_png_bytes(deck.package().part(&media).unwrap()).unwrap()
        };

        let flat = background_of(EraseMode::Flat);
        let inpainted = background_of(EraseMode::Inpaint);

        // 兩者都必須把黑字弄掉
        assert!(
            flat.pixel(175, 100)[0] > 100,
            "Flat 應移除黑字，實際 {:?}",
            flat.pixel(175, 100)
        );
        assert!(
            inpainted.pixel(175, 100)[0] > 100,
            "Inpaint 應移除黑字，實際 {:?}",
            inpainted.pixel(175, 100)
        );

        // 關鍵差異：該處真正的背景是藍色，插值版本應接近它，填色版本會填成白色
        let distance = |a: [u8; 3], b: [u8; 3]| -> i32 {
            a.iter()
                .zip(b.iter())
                .map(|(x, y)| (*x as i32 - *y as i32).abs())
                .max()
                .unwrap()
        };
        let flat_pixel = flat.pixel(175, 100);
        let inpainted_pixel = inpainted.pixel(175, 100);
        assert!(
            distance(inpainted_pixel, blue) < distance(flat_pixel, blue),
            "插值應更接近背景藍：inpaint={inpainted_pixel:?} flat={flat_pixel:?} 藍={blue:?}"
        );
        assert!(
            distance(flat_pixel, blue) > 60,
            "這個情境下 Flat 應該明顯偏離藍色（重現白斑），實際 {flat_pixel:?}"
        );
    }

    /// 在區域內畫出「像文字」的垂直細條紋（實心方塊屬於低頻，不會被遮罩判為筆畫）。
    fn draw_glyph_stripes(image: &mut RgbImage, bbox: BBox, color: [u8; 3]) {
        for y in bbox.top..bbox.bottom {
            for x in bbox.left..bbox.right {
                if (x - bbox.left) % 24 < 3 {
                    image.set_pixel(x as u32, y as u32, color);
                }
            }
        }
    }

    /// 區域平均亮度，用來判斷「筆畫有沒有被擦掉」。
    fn mean_luma(image: &RgbImage, bbox: BBox) -> f64 {
        let mut sum = 0f64;
        let mut count = 0f64;
        for y in bbox.top..bbox.bottom {
            for x in bbox.left..bbox.right {
                let p = image.pixel(x as u32, y as u32);
                sum += 0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64;
                count += 1.0;
            }
        }
        sum / count.max(1.0)
    }

    #[test]
    fn bullet_only_block_is_skipped_and_left_in_background() {
        // 第 7 頁的 `◎`：Vision 把它讀成一個獨立文字區塊。
        // 它應該被跳過（不產生文字框），而且**不能被擦除** —— 原稿的圖示要留在背景。
        let mut bullet = Block::new(BlockKind::Text, BBox::new(100, 100, 120, 130));
        bullet.text = "◎".into();
        bullet.lines = vec![n2p_layout::Line {
            spans: vec![n2p_layout::Span {
                text: Some("◎".into()),
                image_path: None,
            }],
            bbox: Some(bullet.bbox),
        }];
        let mut body = Block::new(BlockKind::Text, BBox::new(200, 100, 380, 140));
        body.text = "差異的根源".into();
        body.lines = vec![n2p_layout::Line {
            spans: vec![n2p_layout::Span {
                text: Some("差異的根源".into()),
                image_path: None,
            }],
            bbox: Some(body.bbox),
        }];

        let layout = LayoutPage {
            size_pt: (400.0, 200.0),
            blocks: vec![bullet, body],
        };
        let mut image = RgbImage::filled(400, 200, [255, 255, 255]);
        // 子彈點位置畫一個紅色圓（模擬原稿的圖示）
        for y in 100..130 {
            for x in 100..120 {
                image.set_pixel(x, y, [200, 40, 40]);
            }
        }
        // 內文位置畫「像文字」的細條紋
        let text_bbox = BBox::new(205, 105, 375, 135);
        draw_glyph_stripes(&mut image, text_bbox, [0, 0, 0]);

        let options = BuildOptions {
            background_resolution: BackgroundResolution::FullImage,
            text_mode: TextMode::Erase,
            ..Default::default()
        };
        let (deck, report) = build_deck(
            &[PageInput {
                layout: &layout,
                image: &image,
            }],
            &options,
        )
        .unwrap();

        assert_eq!(report.pages[0].text_boxes, 1, "只有內文該變成文字框");
        assert_eq!(report.pages[0].skipped_text, 1, "純符號區塊應被跳過");

        let media = deck
            .package()
            .part_names()
            .find(|p| p.starts_with("ppt/media/"))
            .unwrap()
            .to_owned();
        let background = RgbImage::from_png_bytes(deck.package().part(&media).unwrap()).unwrap();

        // 圖示必須原樣保留
        assert_eq!(
            background.pixel(110, 115),
            [200, 40, 40],
            "子彈點圖示不該被擦除"
        );
        // 內文的筆畫必須被擦掉（用平均亮度判斷，單點可能剛好落在條紋間隙）
        assert!(
            mean_luma(&background, text_bbox) > mean_luma(&image, text_bbox) + 20.0,
            "內文筆畫應被擦除：擦除前 {:.0}、擦除後 {:.0}",
            mean_luma(&image, text_bbox),
            mean_luma(&background, text_bbox)
        );
    }

    #[test]
    fn leading_bullet_is_stripped_from_text_but_kept_on_background() {
        let mut block = Block::new(BlockKind::Text, BBox::new(100, 100, 400, 140));
        block.text = "◎ 差異的根源".into();
        block.lines = vec![n2p_layout::Line {
            spans: vec![n2p_layout::Span {
                text: Some(block.text.clone()),
                image_path: None,
            }],
            bbox: Some(block.bbox),
        }];
        let layout = LayoutPage {
            size_pt: (400.0, 200.0),
            blocks: vec![block],
        };
        // 幾何要接近真實比例：子彈點約 1 em、間隙約 0.5 em。
        // 這個區塊的 em 粗估為 (400-100)/5 = 60 px。
        let mut image = RgbImage::filled(400, 200, [255, 255, 255]);
        for y in 100..140 {
            for x in 100..160 {
                image.set_pixel(x, y, [200, 40, 40]); // 子彈點（1 em）
            }
        }
        let text_bbox = BBox::new(190, 100, 395, 140);
        draw_glyph_stripes(&mut image, text_bbox, [0, 0, 0]); // 文字（間隙 0.5 em）

        let options = BuildOptions {
            background_resolution: BackgroundResolution::FullImage,
            text_mode: TextMode::Erase,
            ..Default::default()
        };
        let (deck, _) = build_deck(
            &[PageInput {
                layout: &layout,
                image: &image,
            }],
            &options,
        )
        .unwrap();

        // 文字框不該含子彈點字元
        let names = n2p_ooxml::slide::text_shapes(&deck.slide(0).unwrap()).unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].text, "差異的根源", "行首符號應被剝掉");

        let media = deck
            .package()
            .part_names()
            .find(|p| p.starts_with("ppt/media/"))
            .unwrap()
            .to_owned();
        let background = RgbImage::from_png_bytes(deck.package().part(&media).unwrap()).unwrap();
        assert_eq!(
            background.pixel(130, 120),
            [200, 40, 40],
            "擦除要從文字起點開始，子彈點必須留在背景"
        );
        assert!(
            mean_luma(&background, text_bbox) > mean_luma(&image, text_bbox) + 20.0,
            "文字筆畫應被擦除：擦除前 {:.0}、擦除後 {:.0}",
            mean_luma(&image, text_bbox),
            mean_luma(&background, text_bbox)
        );
    }

    #[test]
    fn cover_mode_leaves_the_background_untouched() {
        // 覆蓋式的架構優點：背景圖完全不動，所以圖示、裝飾不可能被破壞。
        let mut block = Block::new(BlockKind::Text, BBox::new(100, 100, 300, 140));
        block.text = "測試文字".into();
        block.lines = vec![n2p_layout::Line {
            spans: vec![n2p_layout::Span {
                text: Some(block.text.clone()),
                image_path: None,
            }],
            bbox: Some(block.bbox),
        }];
        let layout = LayoutPage {
            size_pt: (400.0, 200.0),
            blocks: vec![block],
        };
        let mut image = RgbImage::filled(400, 200, [255, 255, 255]);
        for y in 100..140 {
            for x in 100..300 {
                image.set_pixel(x, y, [0, 0, 0]);
            }
        }

        let options = BuildOptions {
            background_resolution: BackgroundResolution::FullImage,
            text_mode: TextMode::Cover,
            ..Default::default()
        };
        let (deck, _) = build_deck(
            &[PageInput {
                layout: &layout,
                image: &image,
            }],
            &options,
        )
        .unwrap();

        let media = deck
            .package()
            .part_names()
            .find(|p| p.starts_with("ppt/media/"))
            .unwrap()
            .to_owned();
        let background = RgbImage::from_png_bytes(deck.package().part(&media).unwrap()).unwrap();
        assert_eq!(background, image, "覆蓋式不該改動背景任何一個像素");

        // 但文字框本身要帶底色，否則原稿文字會露出來
        let doc = deck.slide(0).unwrap();
        let sp = doc
            .root
            .path(&["p:sld", "p:cSld", "p:spTree"])
            .unwrap()
            .elements("p:sp")
            .next()
            .unwrap();
        assert!(
            sp.path(&["p:spPr", "a:solidFill"]).is_some(),
            "覆蓋式的文字框必須有不透明底色"
        );
    }

    #[test]
    fn texture_background_gets_invisible_text_instead_of_erasing() {
        // 紋理背景（雜訊）上，筆畫遮罩不可靠 —— 應該改成「透明文字」：
        // 背景一個像素都不動，避免出現灰方塊或雙重文字。
        let mut block = Block::new(BlockKind::Text, BBox::new(100, 100, 300, 140));
        block.text = "測試文字".into();
        block.lines = vec![n2p_layout::Line {
            spans: vec![n2p_layout::Span {
                text: Some(block.text.clone()),
                image_path: None,
            }],
            bbox: Some(block.bbox),
        }];
        let layout = LayoutPage {
            size_pt: (400.0, 200.0),
            blocks: vec![block],
        };

        // 高頻雜訊背景（模擬木紋），文字用「只比背景亮一點」的顏色 ——
        // 這正是 CH7 第 3 頁白字壓在淺色木地板的情況：對比太低，
        // 顏色距離無法分離筆畫。
        let mut image = RgbImage::new(400, 200);
        let mut seed = 12345u32;
        for y in 0..200 {
            for x in 0..400 {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let v = ((seed >> 16) as u8) / 3 + 60; // 60..145
                image.set_pixel(x, y, [v, v, v]);
            }
        }
        draw_glyph_stripes(&mut image, BBox::new(100, 100, 300, 140), [70, 70, 70]);
        let before = image.clone();

        let options = BuildOptions {
            background_resolution: BackgroundResolution::FullImage,
            text_mode: TextMode::Erase,
            ..Default::default()
        };
        let (deck, _) = build_deck(
            &[PageInput {
                layout: &layout,
                image: &image,
            }],
            &options,
        )
        .unwrap();

        let media = deck
            .package()
            .part_names()
            .find(|p| p.starts_with("ppt/media/"))
            .unwrap()
            .to_owned();
        let background = RgbImage::from_png_bytes(deck.package().part(&media).unwrap()).unwrap();
        if background != before {
            let differing = background
                .data
                .iter()
                .zip(before.data.iter())
                .filter(|(a, b)| a != b)
                .count();
            eprintln!(
                "media={media} 尺寸 {:?} vs {:?}，不同位元組 {}",
                (background.width, background.height),
                (before.width, before.height),
                differing
            );
        }
        assert_eq!(background, before, "紋理背景不該被改動");

        // 文字框必須存在，但文字是全透明的
        let doc = deck.slide(0).unwrap();
        let x = n2p_ooxml::xml::to_bytes(&doc).unwrap();
        let xml = String::from_utf8_lossy(&x);
        assert!(xml.contains("<a:alpha val=\"0\"/>"), "文字應設為全透明");
    }

    #[test]
    fn empty_input_is_rejected() {
        assert!(matches!(
            build_deck(&[], &BuildOptions::default()),
            Err(Error::NoPages)
        ));
    }

    #[test]
    fn full_image_background_keeps_resolution() {
        let layout = page();
        let image = RgbImage::filled(1920, 1080, [1, 2, 3]);
        let options = BuildOptions {
            background_resolution: BackgroundResolution::FullImage,
            erase_background_regions: false,
            ..Default::default()
        };
        let (deck, _) = build_deck(
            &[PageInput {
                layout: &layout,
                image: &image,
            }],
            &options,
        )
        .unwrap();
        let media = deck
            .package()
            .part_names()
            .find(|p| p.starts_with("ppt/media/"))
            .expect("應有背景影像")
            .to_owned();
        let bytes = deck.package().part(&media).unwrap();
        let decoded = RgbImage::from_png_bytes(bytes).unwrap();
        assert_eq!((decoded.width, decoded.height), (1920, 1080));
    }
}
