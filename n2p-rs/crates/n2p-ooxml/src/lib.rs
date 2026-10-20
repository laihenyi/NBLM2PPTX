//! # n2p-ooxml
//!
//! NotebookLM2PPT 的 Rust 改寫專案中，用來取代 `spire.presentation` 的
//! 最小 PPTX/OOXML 寫入層。
//!
//! ## 範圍
//!
//! 只實作本專案實際需要的操作，不追求通用 PPTX 函式庫：
//!
//! | 模組 | 職責 | 對應的 Spire 呼叫 |
//! | :--- | :--- | :--- |
//! | [`opc`] | OPC 封裝（zip + `[Content_Types].xml` + `_rels`） | （容器層） |
//! | [`emu`] | 點 ↔ EMU 換算、畫布尺寸 | `SlideSize`、`RectangleF` |
//! | [`shape`] | `<p:sp>` 矩形文字框 | `Shapes.AppendShape` |
//! | [`picture`] | `<p:pic>` 圖片 | `Shapes.AppendEmbedImageByPath` |
//! | [`background`] | `<p:bg>` 背景填滿 | `SlideBackground.Fill` |
//! | [`slide`] | `spTree` 插入／置底／依名稱刪除 | `Shapes` 集合操作 |
//! | [`pres`] | 畫布尺寸、投影片清單 | `Presentation.SlideSize` |
//! | [`deck`] | 高階門面 | — |
//!
//! ## 刻意偏離 Spire 的三點
//!
//! 1. **不產生任何浮水印。** Python 版的 `clean_ppt()` 其實是在刪 Spire 評估版的
//!    水印形狀；本 crate 自己寫 XML，因此不需要該步驟。
//! 2. **合併時共用 master／layout。** Spire 的 `AppendBySlide` 會為每份來源各帶
//!    一整套 master + layout + theme（3 份同模板 → 3 套）。這裡採同構假設，
//!    只搬 `slideN.xml` 與其 media。
//! 3. **`docProps/app.xml` 寫對。** Spire 會把 16:9 畫布寫成「全屏显示(4:3)」、
//!    把 3 頁簡報寫成 `Slides=1`；本 crate 不重現這兩個 bug。
//!
//! ## 範例
//!
//! ```no_run
//! use n2p_ooxml::{Deck, Rect, SlideSize, shape::{TextBox, Align}};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut deck = Deck::open("template.pptx")?;
//! deck.set_slide_size(SlideSize::Screen16x9)?;
//!
//! let text = TextBox::new("Block_text", "Hello MinerU", Rect::ltrb(60.0, 40.0, 660.0, 120.0))
//!     .align(Align::Center);
//! deck.push_text_box(0, &text)?;
//!
//! let png = std::fs::read("page.png")?;
//! deck.set_background_picture(0, png.clone(), "png")?;
//! deck.insert_picture_at_bottom(0, png, "png", Rect::ltrb(60.0, 40.0, 400.0, 300.0), "fg")?;
//!
//! deck.save("out.pptx")?;
//! # Ok(())
//! # }
//! ```

pub mod background;
pub mod deck;
pub mod emu;
pub mod error;
pub mod merge;
pub mod opc;
pub mod picture;
pub mod pres;
pub mod props;
pub mod shape;
pub mod slide;
pub mod xml;

pub use background::Background;
pub use deck::Deck;
pub use emu::{Rect, SlideSize, centipoint_from_pt, emu_from_pt, pt_from_emu};
pub use error::{Error, Result};
pub use merge::{AppendOutcome, MergeOptions, MergeReport};
pub use opc::{Package, Relationship, Relationships};
pub use picture::Picture;
pub use shape::{Align, Anchor, AutoFit, NewlineMode, TextBox};
