//! 在同一行程內連續轉換，觀察第二次是否成功、記憶體是否釋放。
//!
//! ```text
//! cargo run --release -p n2p-gui --example twice -- <pdf> <out-dir>
//! ```

use std::path::PathBuf;

fn rss_mb() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
        / 1024
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pdf = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[2]);
    println!("啟動 RSS {} MB", rss_mb());
    for (round, extract) in [(1, false), (2, true), (3, false)] {
        let mut options = n2p_pipeline::ConvertOptions::default();
        options.build.extract_objects = extract;
        options.build.erase_mode = n2p_build::EraseMode::Lama;
        options.build.background_resolution = n2p_build::BackgroundResolution::FullImage;
        let out = out_dir.join(format!("twice_{round}.pptx"));
        let started = std::time::Instant::now();
        let thread = std::thread::spawn(move || {
            n2p_pipeline::convert_with_progress(&pdf_clone(), &out, &options, &mut |_| true)
        });
        fn pdf_clone() -> PathBuf {
            PathBuf::from(std::env::args().nth(1).unwrap())
        }
        let report = thread.join().unwrap().expect("轉換失敗");
        let before_release = rss_mb();
        if std::env::var_os("N2P_RELEASE").is_some() {
            n2p_pipeline::release_models();
        }
        print!("釋放前 {before_release} MB；");
        println!(
            "第 {round} 次（物件={extract}）：{:.0} 秒，文字框 {}、圖片 {}，結束後 RSS {} MB",
            started.elapsed().as_secs_f64(),
            report.text_boxes,
            report.pictures,
            rss_mb()
        );
    }
    let _ = pdf;
}
