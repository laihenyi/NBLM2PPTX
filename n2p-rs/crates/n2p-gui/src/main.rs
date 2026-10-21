//! `n2p-gui`：把 NotebookLM 匯出的 PDF 轉成可編輯 PowerPoint 的圖形介面。
//!
//! 與命令列 `n2p convert` 走同一條 pipeline，速度與記憶體需求相同。

mod activity;
mod app;
mod debug;
mod model;
mod worker;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("NotebookLM 轉 PPT")
            .with_inner_size([720.0, 680.0])
            .with_min_inner_size([560.0, 480.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "NotebookLM 轉 PPT",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, debug::DebugHooks::from_env())))),
    )
}
