//! 量測各階段耗時：cargo run --release -p n2p-gui --example timing -- <pdf> <pages> <lama|stroke> [objects]
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut options = n2p_pipeline::ConvertOptions {
        pages: Some(n2p_pipeline::parse_page_range(&args[2]).unwrap()),
        ..Default::default()
    };
    options.build.background_resolution = n2p_build::BackgroundResolution::FullImage;
    options.build.erase_mode = if args[3] == "lama" {
        n2p_build::EraseMode::Lama
    } else {
        n2p_build::EraseMode::StrokeMask
    };
    options.build.extract_objects = args.get(4).is_some_and(|a| a == "objects");
    let out = std::env::temp_dir().join("n2p-timing.pptx");
    let start = Instant::now();
    let mut last = start;
    let mut last_label = String::from("開檔");
    n2p_pipeline::convert_with_progress(std::path::Path::new(&args[1]), &out, &options, &mut |p| {
        let now = Instant::now();
        println!("{:>7.1}s  {last_label}", (now - last).as_secs_f64());
        last = now;
        last_label = format!("{p:?}");
        true
    })
    .unwrap();
    println!("{:>7.1}s  {last_label}", last.elapsed().as_secs_f64());
    println!("總計 {:.1}s", start.elapsed().as_secs_f64());
}
