//! 轉換 LaMa 模型並印出節點統計：cargo run --release -p n2p-image --example gpu_convert -- <原模型> <輸出>
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let started = Instant::now();
    let stats = n2p_image::gpu::convert(args[1].as_ref(), args[2].as_ref()).unwrap();
    println!("轉換 {:.1}s {stats:?}", started.elapsed().as_secs_f64());
    for (op, count) in n2p_image::gpu::op_histogram_of(args[2].as_ref()).unwrap() {
        print!("{op}:{count} ");
    }
    println!();
}
