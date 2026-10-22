//! `n2p-gui`：把 NotebookLM 匯出的 PDF 轉成可編輯 PowerPoint 的圖形介面。
//!
//! 與命令列 `n2p convert` 走同一條 pipeline，速度與記憶體需求相同。

mod activity;
mod app;
mod debug;
mod i18n;
mod model;
mod worker;

fn main() -> eframe::Result {
    #[cfg(all(feature = "appstore", not(feature = "sandbox-test")))]
    clear_developer_overrides();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("NBLM2PPTX")
            .with_inner_size(debug::window_size().unwrap_or([720.0, 680.0]))
            .with_min_inner_size([560.0, 480.0])
            .with_drag_and_drop(true)
            // 空圖示：eframe 預設會在啟動後把 Dock 圖示換成 egui 標誌，
            // 給空的就不覆蓋，沿用 bundle 的 AppIcon.icns
            .with_icon(eframe::egui::IconData::default()),
        ..Default::default()
    };
    eframe::run_native(
        "NBLM2PPTX",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, debug::DebugHooks::from_env())))),
    )
}

/// App Store 版不接受任何 `N2P_*` 環境變數：模型路徑、推論裝置、外擴量、除錯掛鉤
/// 都是開發用的覆寫，審核規範 2.5.19 不允許未載明的功能，所以啟動時一律清掉。
#[cfg(all(feature = "appstore", not(feature = "sandbox-test")))]
fn clear_developer_overrides() {
    let names: Vec<String> = std::env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .filter(|name| name.starts_with("N2P_"))
        .collect();
    for name in names {
        // SAFETY: 在 main 一開始執行，此時還沒有其他執行緒讀寫環境變數。
        unsafe { std::env::remove_var(name) };
    }
}
