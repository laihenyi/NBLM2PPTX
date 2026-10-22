//! 與 Python（numpy）參考實作的逐像素比對。
//!
//! 基準由 `fixtures/make_image_fixtures.py` 產生：
//! 同一張合成影像、同一個矩形、同一組參數，跑 Python 版演算法。
//!
//! `background`、`background_smooth`、`edge_mean_smooth`、`onion` 四種應**完全相同**；
//! `griddata` 與 `skimage` 是替代演算法，只驗證合理性。

use std::path::PathBuf;

use n2p_image::{
    BBox, InpaintMethod, RgbImage, edge_average_color, edge_diversity, inpaint_region,
};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/image-fixtures")
}

fn required(name: &str) -> PathBuf {
    let path = fixtures().join(name);
    assert!(path.exists(), "缺少 fixture {path:?}");
    path
}

fn load_input() -> RgbImage {
    RgbImage::open(required("input.png")).expect("讀取 input.png")
}

fn metrics() -> serde_json::Value {
    let text = std::fs::read_to_string(required("expected_metrics.json")).expect("讀取 metrics");
    serde_json::from_str(&text).expect("解析 metrics")
}

fn box_from(json: &serde_json::Value) -> BBox {
    let b = &json["box"];
    BBox::new(
        b["left"].as_i64().unwrap(),
        b["top"].as_i64().unwrap(),
        b["right"].as_i64().unwrap(),
        b["bottom"].as_i64().unwrap(),
    )
}

/// 逐像素差異統計：`(最大通道差, 不同的像素數, 總像素數)`。
fn diff(actual: &RgbImage, expected: &RgbImage) -> (u8, usize, usize) {
    assert_eq!(
        (actual.width, actual.height),
        (expected.width, expected.height),
        "尺寸不同"
    );
    let mut max_channel = 0u8;
    let mut differing_pixels = 0usize;
    for (a, e) in actual.data.iter().zip(expected.data.iter()) {
        max_channel = max_channel.max(a.abs_diff(*e));
    }
    for (a, e) in actual
        .data
        .chunks_exact(3)
        .zip(expected.data.chunks_exact(3))
    {
        if a != e {
            differing_pixels += 1;
        }
    }
    (max_channel, differing_pixels, actual.pixel_count())
}

#[test]
fn edge_diversity_matches_numpy_exactly() {
    let image = load_input();
    let json = metrics();
    let bbox = box_from(&json);

    let (diversity, fill) = edge_diversity(&image, bbox, 15);
    let expected_diversity = json["edge_diversity_default"].as_f64().unwrap();
    assert_eq!(diversity, expected_diversity, "多樣性必須完全相同");

    let expected_fill: Vec<u8> = json["edge_diversity_fill_color"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect();
    assert_eq!(fill.to_vec(), expected_fill, "主色必須完全相同");

    // tolerance = 20（refine 流程用的是這個值）
    let (diversity20, fill20) = edge_diversity(&image, bbox, 20);
    assert_eq!(
        diversity20,
        json["edge_diversity_tolerance20"].as_f64().unwrap()
    );
    let expected20: Vec<u8> = json["edge_diversity_tolerance20_fill_color"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect();
    assert_eq!(fill20.to_vec(), expected20);
}

#[test]
fn edge_average_color_matches_numpy_exactly() {
    let image = load_input();
    let json = metrics();
    let average = edge_average_color(&image, box_from(&json));
    let expected: Vec<u8> = json["edge_average_color"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect();
    assert_eq!(average.to_vec(), expected);
}

fn assert_bit_exact(method: InpaintMethod, fixture: &str) {
    let image = load_input();
    let json = metrics();
    let bbox = box_from(&json);
    let actual = inpaint_region(&image, bbox, method);
    let expected = RgbImage::open(required(fixture)).expect("讀取參考輸出");
    let (max_channel, differing, total) = diff(&actual, &expected);
    assert_eq!(
        differing, 0,
        "{method}：有 {differing}/{total} 個像素不同（最大通道差 {max_channel}）"
    );
}

#[test]
fn background_matches_numpy_exactly() {
    assert_bit_exact(InpaintMethod::Background, "expected_background.png");
}

#[test]
fn background_smooth_matches_numpy_exactly() {
    assert_bit_exact(
        InpaintMethod::BackgroundSmooth,
        "expected_background_smooth.png",
    );
}

#[test]
fn edge_mean_smooth_matches_numpy_exactly() {
    assert_bit_exact(
        InpaintMethod::EdgeMeanSmooth,
        "expected_edge_mean_smooth.png",
    );
}

#[test]
fn onion_matches_numpy_exactly() {
    assert_bit_exact(InpaintMethod::Onion, "expected_onion.png");
}

#[test]
fn bit_exact_claims_match_what_we_verified() {
    // 這兩個測試是在斷言「哪些方法逐位元等價」這件事本身，
    // 避免有人改了演算法卻忘了更新文件上的說法。
    for method in InpaintMethod::all() {
        if method.is_bit_exact_with_python() {
            assert!(
                matches!(
                    method,
                    InpaintMethod::Background
                        | InpaintMethod::BackgroundSmooth
                        | InpaintMethod::EdgeMeanSmooth
                        | InpaintMethod::Onion
                ),
                "{method} 不該被標記為逐位元等價"
            );
        }
    }
}

#[test]
fn replacement_methods_produce_plausible_output() {
    let image = load_input();
    let json = metrics();
    let bbox = box_from(&json);
    let expected = RgbImage::open(required("expected_background_smooth.png")).unwrap();

    for method in [InpaintMethod::Idw, InpaintMethod::Harmonic] {
        let actual = inpaint_region(&image, bbox, method);
        let (max_channel, _, total) = diff(&actual, &expected);
        // 替代演算法不要求逐位元相同，但要落在同一視覺範圍內。
        assert!(
            max_channel < 40,
            "{method}：與參考結果最大通道差 {max_channel}（{total} 像素）過大"
        );
        // 遮罩外的像素不得被動到（這是所有方法共通的硬需求）
        for y in 0..image.height {
            for x in 0..image.width {
                let inside = x >= bbox.left as u32
                    && x < bbox.right as u32
                    && y >= bbox.top as u32
                    && y < bbox.bottom as u32;
                if !inside {
                    assert_eq!(
                        actual.pixel(x, y),
                        image.pixel(x, y),
                        "{method}：({x},{y}) 在遮罩外卻被改動"
                    );
                }
            }
        }
    }
}

#[test]
fn bit_exact_methods_never_touch_pixels_outside_the_box() {
    let image = load_input();
    let json = metrics();
    let bbox = box_from(&json);

    for method in [
        InpaintMethod::Background,
        InpaintMethod::BackgroundSmooth,
        InpaintMethod::EdgeMeanSmooth,
        InpaintMethod::Onion,
    ] {
        let actual = inpaint_region(&image, bbox, method);
        for y in 0..image.height {
            for x in 0..image.width {
                let inside = x >= bbox.left as u32
                    && x < bbox.right as u32
                    && y >= bbox.top as u32
                    && y < bbox.bottom as u32;
                if !inside {
                    assert_eq!(actual.pixel(x, y), image.pixel(x, y), "{method}：({x},{y})");
                }
            }
        }
    }
}
