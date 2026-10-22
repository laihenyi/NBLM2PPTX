//! LaMa 的 GPU 路徑：把原始模型改寫成 CoreML 能整張接手的形式。
//!
//! 原始 `lama_fp32.onnx` 直接交給 CoreML 時，大半節點會退回 CPU，來回搬資料
//! 反而比純 CPU 慢。原因不在模型本身，而在匯出方式：
//!
//! - batch 維度是變數，形狀計算留在執行期（17,480 個節點）
//! - FFT 用 `Einsum(ij,jk->ik)` 寫成，CoreML 不支援
//! - 純量索引的 `Gather` 作用在 5 維張量上，CoreML 只接受 4 維以下
//! - `Neg` 與帶 `output_padding` 的 `ConvTranspose` 也不被接受
//!
//! 轉換步驟（全部是數學等價改寫）：
//!
//! 1. batch 維度固定為 1
//! 2. 交給 ONNX Runtime 做基本層級最佳化（常數摺疊），存出 2,841 個節點的圖
//! 3. `Einsum(ij,jk->ik)` → `MatMul`；`Neg` → `Mul(-1)`；
//!    純量 `Gather` → `Slice` + `Squeeze`；
//!    `ConvTranspose` 的 `output_padding`／不對稱 `pads` → 不裁切的完整輸出 + `Slice`
//!
//! 改寫後 2,916 個節點全部落在 CoreML 的單一分區。M 系列晶片實測每個 512 圖塊
//! 0.20 秒（純 CPU 1.1 秒），輸出與 CPU 最大差 0.0006（0–255 尺度）。

use std::path::{Path, PathBuf};

use ort::session::Session;
use ort::session::builder::{AutoDevicePolicy, GraphOptimizationLevel};
use protobuf::{Enum, EnumOrUnknown, Message};

use crate::onnx_pb::onnx::{
    AttributeProto, ModelProto, NodeProto, TensorProto, attribute_proto::AttributeType,
    tensor_proto::DataType,
};

/// 改寫邏輯的版本。改寫規則一變就要遞增，讓舊的轉換檔與 CoreML 編譯快取失效。
pub const CONVERTER_VERSION: u32 = 1;

/// 轉換檔的位置：原模型旁，檔名帶轉換器版本。
pub fn converted_path(original: &Path) -> PathBuf {
    let stem = original
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "lama".into());
    original.with_file_name(format!("{stem}.coreml-v{CONVERTER_VERSION}.onnx"))
}

/// CoreML 編譯快取目錄。CoreML 的快取只看圖結構、不看權重，所以目錄必須跟著轉換器版本走。
pub fn compile_cache_dir(original: &Path) -> PathBuf {
    original
        .with_file_name("coreml")
        .join(format!("v{CONVERTER_VERSION}"))
}

/// 轉換結果的統計，用來確認改寫器真的把該換的節點都換掉了。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RewriteStats {
    pub einsum: usize,
    pub neg: usize,
    pub gather: usize,
    pub conv_transpose: usize,
}

/// 若轉換檔不存在，從 `original` 產生它。回傳轉換檔路徑。
///
/// 先寫到暫存檔再改名：中途失敗不會留下半個模型。暫存檔名帶行程編號，
/// 兩個行程同時轉換時各寫各的，最後改名的那份勝出（內容相同）。
pub fn ensure_converted(original: &Path) -> Result<PathBuf, String> {
    let target = converted_path(original);
    if target.exists() {
        return Ok(target);
    }
    let partial = target.with_extension(format!("onnx.{}.partial", std::process::id()));
    let result = convert(original, &partial).and_then(|_| {
        std::fs::rename(&partial, &target)
            .map_err(|e| format!("無法寫入 {}：{e}", target.display()))
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result.map(|()| target)
}

/// 把 `original` 改寫後寫到 `output`。
pub fn convert(original: &Path, output: &Path) -> Result<RewriteStats, String> {
    let bytes =
        std::fs::read(original).map_err(|e| format!("無法讀取 {}：{e}", original.display()))?;
    let mut model =
        ModelProto::parse_from_bytes(&bytes).map_err(|e| format!("模型解析失敗：{e}"))?;
    drop(bytes);
    fix_dim_param(&mut model, "batch", 1);
    let fixed = model.write_to_bytes().map_err(|e| e.to_string())?;
    drop(model);

    // ORT 只能把最佳化結果寫成檔案，所以借用輸出路徑當中繼
    {
        let environment = crate::runtime::environment()?;
        let failed = |e: &dyn std::fmt::Display| format!("無法建立最佳化 session：{e}");
        // ort 在 macOS 預設會自動掛上 CoreML，留下無法存檔的已編譯節點；摺疊必須純 CPU
        let mut builder = Session::builder(environment)
            .map_err(|e| failed(&e))?
            .with_auto_device(AutoDevicePolicy::PreferCPU)
            .map_err(|e| failed(&e))?
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .map_err(|e| failed(&e))?
            .with_optimized_model_path(output)
            .map_err(|e| failed(&e))?;
        // session 只是為了讓 ORT 存出最佳化後的圖，建完立刻丟掉
        builder
            .commit_from_memory(&fixed)
            .map_err(|e| format!("常數摺疊失敗：{e}"))?;
    }
    drop(fixed);

    let folded = std::fs::read(output).map_err(|e| format!("讀不到最佳化結果：{e}"))?;
    let mut model =
        ModelProto::parse_from_bytes(&folded).map_err(|e| format!("最佳化結果解析失敗：{e}"))?;
    drop(folded);
    let stats = rewrite_for_coreml(&mut model);
    let bytes = model.write_to_bytes().map_err(|e| e.to_string())?;
    std::fs::write(output, bytes).map_err(|e| format!("無法寫入 {}：{e}", output.display()))?;
    Ok(stats)
}

/// 把輸入、輸出與中間值宣告裡名為 `name` 的符號維度固定成 `value`。
pub fn fix_dim_param(model: &mut ModelProto, name: &str, value: i64) {
    let Some(graph) = model.graph.as_mut() else {
        return;
    };
    let infos = graph
        .input
        .iter_mut()
        .chain(graph.output.iter_mut())
        .chain(graph.value_info.iter_mut());
    for info in infos {
        let Some(ty) = info.type_.as_mut() else {
            continue;
        };
        if !ty.has_tensor_type() {
            continue;
        }
        let Some(shape) = ty.mut_tensor_type().shape.as_mut() else {
            continue;
        };
        for dim in &mut shape.dim {
            if dim.has_dim_param() && dim.dim_param() == name {
                dim.set_dim_value(value);
            }
        }
    }
}

/// 把 CoreML 不支援的節點換成等價寫法。不符合條件的節點原樣保留。
pub fn rewrite_for_coreml(model: &mut ModelProto) -> RewriteStats {
    let opset = model
        .opset_import
        .iter()
        .find(|o| o.domain.is_empty() || o.domain == "ai.onnx")
        .map_or(0, |o| o.version);
    let mut stats = RewriteStats::default();
    let Some(graph) = model.graph.as_mut() else {
        return stats;
    };
    let constants = scalar_constants(&graph.initializer, &graph.node);
    let mut extra = Vec::new();
    let mut nodes = Vec::with_capacity(graph.node.len());
    let minus_one = unique_name(&graph.initializer, "n2p_minus_one");
    let mut need_minus_one = false;

    for mut node in std::mem::take(&mut graph.node) {
        if !node.domain.is_empty() && node.domain != "ai.onnx" {
            nodes.push(node);
            continue;
        }
        match node.op_type.as_str() {
            "Einsum" if string_attr(&node, "equation").as_deref() == Some("ij,jk->ik") => {
                node.op_type = "MatMul".into();
                node.attribute.clear();
                stats.einsum += 1;
                nodes.push(node);
            }
            "Neg" if opset >= 7 => {
                node.op_type = "Mul".into();
                node.input.push(minus_one.clone());
                need_minus_one = true;
                stats.neg += 1;
                nodes.push(node);
            }
            "Gather" if opset >= 13 && node.input.len() == 2 => {
                let Some(&index) = constants.get(&node.input[1]) else {
                    nodes.push(node);
                    continue;
                };
                let axis = int_attr(&node, "axis").unwrap_or(0);
                let base = base_name(&node);
                let end = if index == -1 { i64::MAX } else { index + 1 };
                let (start_name, end_name, axis_name) = (
                    format!("{base}_n2p_start"),
                    format!("{base}_n2p_end"),
                    format!("{base}_n2p_axis"),
                );
                extra.push(int64_tensor(&start_name, &[index]));
                extra.push(int64_tensor(&end_name, &[end]));
                extra.push(int64_tensor(&axis_name, &[axis]));
                let sliced = format!("{base}_n2p_sliced");
                nodes.push(make_node(
                    "Slice",
                    &format!("{base}_n2p_slice"),
                    vec![
                        node.input[0].clone(),
                        start_name,
                        end_name,
                        axis_name.clone(),
                    ],
                    vec![sliced.clone()],
                ));
                nodes.push(make_node(
                    "Squeeze",
                    &format!("{base}_n2p_squeeze"),
                    vec![sliced, axis_name],
                    node.output.clone(),
                ));
                stats.gather += 1;
            }
            "ConvTranspose" => match conv_transpose_crop(&node) {
                None => nodes.push(node),
                Some(Crop::Symmetric(pads)) => {
                    node.attribute
                        .retain(|a| a.name != "output_padding" && a.name != "pads");
                    node.attribute.push(ints_attr("pads", &pads));
                    stats.conv_transpose += 1;
                    nodes.push(node);
                }
                Some(Crop::Slice { begin, end }) => {
                    let base = base_name(&node);
                    let full = format!("{base}_n2p_full");
                    let output = std::mem::replace(&mut node.output[0], full.clone());
                    node.attribute
                        .retain(|a| a.name != "output_padding" && a.name != "pads");
                    node.attribute
                        .push(ints_attr("pads", &vec![0; begin.len() * 2]));
                    nodes.push(node);
                    let ends: Vec<i64> = end
                        .iter()
                        .map(|&e| if e > 0 { -e } else { i64::MAX })
                        .collect();
                    let axes: Vec<i64> = (2..2 + begin.len() as i64).collect();
                    let names = [
                        format!("{base}_n2p_crop_start"),
                        format!("{base}_n2p_crop_end"),
                        format!("{base}_n2p_crop_axes"),
                    ];
                    extra.push(int64_tensor(&names[0], &begin));
                    extra.push(int64_tensor(&names[1], &ends));
                    extra.push(int64_tensor(&names[2], &axes));
                    let mut inputs = vec![full];
                    inputs.extend(names);
                    nodes.push(make_node(
                        "Slice",
                        &format!("{base}_n2p_crop"),
                        inputs,
                        vec![output],
                    ));
                    stats.conv_transpose += 1;
                }
            },
            _ => nodes.push(node),
        }
    }
    graph.node = nodes;
    if need_minus_one {
        let mut tensor = TensorProto::new();
        tensor.name = minus_one;
        tensor.data_type = DataType::FLOAT as i32;
        tensor.float_data = vec![-1.0];
        graph.initializer.push(tensor);
    }
    graph.initializer.extend(extra);
    stats
}

/// 模型檔的節點型別統計（除錯與測試用）。
pub fn op_histogram_of(path: &Path) -> Result<std::collections::BTreeMap<String, usize>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let model = ModelProto::parse_from_bytes(&bytes).map_err(|e| e.to_string())?;
    Ok(op_histogram(&model))
}

/// 節點型別的統計。
pub fn op_histogram(model: &ModelProto) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for node in &model.graph.node {
        *counts.entry(node.op_type.clone()).or_insert(0) += 1;
    }
    counts
}

enum Crop {
    /// 改寫後 pads 已對稱，CoreML 可直接接受。
    Symmetric(Vec<i64>),
    /// 不裁切地算出完整輸出，再從各空間軸的開頭裁 `begin`、結尾裁 `end` 格。
    Slice { begin: Vec<i64>, end: Vec<i64> },
}

/// `output_padding` 等於結尾少裁幾格，所以先併進 pads；pads 仍不對稱就改用 Slice 裁切。
/// 其他 CoreML 不在意的情況回傳 `None`（不動）。
fn conv_transpose_crop(node: &NodeProto) -> Option<Crop> {
    if node.attribute.iter().any(|a| a.name == "output_shape")
        || string_attr(node, "auto_pad").is_some_and(|p| p != "NOTSET")
    {
        return None;
    }
    let output_padding = ints(node, "output_padding").unwrap_or_default();
    let has_output_padding = output_padding.iter().any(|&p| p != 0);
    let spatial = ints(node, "kernel_shape")
        .map(|k| k.len())
        .or_else(|| ints(node, "pads").map(|p| p.len() / 2))
        .or(Some(output_padding.len()).filter(|&n| n > 0))?;
    let mut pads = ints(node, "pads").unwrap_or_else(|| vec![0; spatial * 2]);
    if pads.len() != spatial * 2 {
        return None;
    }
    for (axis, &extra) in output_padding.iter().enumerate().take(spatial) {
        pads[spatial + axis] -= extra;
    }
    if pads.iter().any(|&p| p < 0) {
        return None;
    }
    if pads.iter().all(|&p| p == pads[0]) {
        return has_output_padding.then_some(Crop::Symmetric(pads));
    }
    Some(Crop::Slice {
        begin: pads[..spatial].to_vec(),
        end: pads[spatial..].to_vec(),
    })
}

/// 找出所有純量整數常數（initializer 或 Constant 節點）。
fn scalar_constants(
    initializers: &[TensorProto],
    nodes: &[NodeProto],
) -> std::collections::HashMap<String, i64> {
    let mut found = std::collections::HashMap::new();
    for tensor in initializers {
        if let Some(value) = scalar_int(tensor) {
            found.insert(tensor.name.clone(), value);
        }
    }
    for node in nodes {
        if node.op_type != "Constant" || node.output.len() != 1 {
            continue;
        }
        let tensor = node
            .attribute
            .iter()
            .find(|a| a.name == "value")
            .and_then(|a| a.t.as_ref());
        if let Some(value) = tensor.and_then(scalar_int) {
            found.insert(node.output[0].clone(), value);
        }
    }
    found
}

fn scalar_int(tensor: &TensorProto) -> Option<i64> {
    if !tensor.dims.is_empty() {
        return None;
    }
    match DataType::from_i32(tensor.data_type)? {
        DataType::INT64 => tensor.int64_data.first().copied().or_else(|| {
            let raw: [u8; 8] = tensor.raw_data.get(..8)?.try_into().ok()?;
            Some(i64::from_le_bytes(raw))
        }),
        DataType::INT32 => tensor
            .int32_data
            .first()
            .map(|&v| i64::from(v))
            .or_else(|| {
                let raw: [u8; 4] = tensor.raw_data.get(..4)?.try_into().ok()?;
                Some(i64::from(i32::from_le_bytes(raw)))
            }),
        _ => None,
    }
}

fn attr<'a>(node: &'a NodeProto, name: &str) -> Option<&'a AttributeProto> {
    node.attribute.iter().find(|a| a.name == name)
}

fn string_attr(node: &NodeProto, name: &str) -> Option<String> {
    attr(node, name).map(|a| String::from_utf8_lossy(&a.s).into_owned())
}

fn int_attr(node: &NodeProto, name: &str) -> Option<i64> {
    attr(node, name).map(|a| a.i)
}

fn ints(node: &NodeProto, name: &str) -> Option<Vec<i64>> {
    attr(node, name).map(|a| a.ints.clone())
}

fn ints_attr(name: &str, values: &[i64]) -> AttributeProto {
    let mut attribute = AttributeProto::new();
    attribute.name = name.into();
    attribute.type_ = EnumOrUnknown::new(AttributeType::INTS);
    attribute.ints = values.to_vec();
    attribute
}

fn int64_tensor(name: &str, values: &[i64]) -> TensorProto {
    let mut tensor = TensorProto::new();
    tensor.name = name.into();
    tensor.data_type = DataType::INT64 as i32;
    tensor.dims = vec![values.len() as i64];
    tensor.int64_data = values.to_vec();
    tensor
}

fn make_node(op: &str, name: &str, input: Vec<String>, output: Vec<String>) -> NodeProto {
    let mut node = NodeProto::new();
    node.op_type = op.into();
    node.name = name.into();
    node.input = input;
    node.output = output;
    node
}

fn base_name(node: &NodeProto) -> String {
    if node.name.is_empty() {
        node.output[0].clone()
    } else {
        node.name.clone()
    }
}

fn unique_name(initializers: &[TensorProto], wanted: &str) -> String {
    let mut name = wanted.to_string();
    let mut n = 1;
    while initializers.iter().any(|t| t.name == name) {
        name = format!("{wanted}_{n}");
        n += 1;
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onnx_pb::onnx::{
        GraphProto, OperatorSetIdProto, TensorShapeProto, TypeProto, ValueInfoProto, type_proto,
    };
    use ort::value::Tensor;
    use protobuf::MessageField;

    fn value_info(name: &str, shape: &[i64]) -> ValueInfoProto {
        let mut dims = TensorShapeProto::new();
        for &d in shape {
            let mut dim = crate::onnx_pb::onnx::tensor_shape_proto::Dimension::new();
            if d < 0 {
                dim.set_dim_param("batch".into());
            } else {
                dim.set_dim_value(d);
            }
            dims.dim.push(dim);
        }
        let mut tensor = type_proto::Tensor::new();
        tensor.elem_type = DataType::FLOAT as i32;
        tensor.shape = MessageField::some(dims);
        let mut ty = TypeProto::new();
        ty.set_tensor_type(tensor);
        let mut info = ValueInfoProto::new();
        info.name = name.into();
        info.type_ = MessageField::some(ty);
        info
    }

    fn float_tensor(name: &str, dims: &[i64], seed: u32) -> TensorProto {
        let mut tensor = TensorProto::new();
        tensor.name = name.into();
        tensor.data_type = DataType::FLOAT as i32;
        tensor.dims = dims.to_vec();
        tensor.float_data = random(dims.iter().product::<i64>() as usize, seed);
        tensor
    }

    fn random(len: usize, seed: u32) -> Vec<f32> {
        let mut state = seed.wrapping_mul(2_654_435_761).max(1);
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state % 2001) as f32 / 1000.0 - 1.0
            })
            .collect()
    }

    /// 單一輸入 `x`、單一輸出 `y` 的模型。
    fn model(nodes: Vec<NodeProto>, inits: Vec<TensorProto>, x: &[i64], y: &[i64]) -> ModelProto {
        let mut graph = GraphProto::new();
        graph.name = "t".into();
        graph.node = nodes;
        graph.initializer = inits;
        graph.input.push(value_info("x", x));
        graph.output.push(value_info("y", y));
        let mut model = ModelProto::new();
        model.ir_version = 8;
        let mut opset = OperatorSetIdProto::new();
        opset.version = 17;
        model.opset_import.push(opset);
        model.graph = MessageField::some(graph);
        model
    }

    fn run(model: &ModelProto, shape: &[usize]) -> Vec<f32> {
        let environment = crate::runtime::environment().unwrap();
        let bytes = model.write_to_bytes().unwrap();
        let mut session = Session::builder(environment)
            .unwrap()
            .with_auto_device(AutoDevicePolicy::PreferCPU)
            .unwrap()
            .commit_from_memory(&bytes)
            .unwrap();
        let len = shape.iter().product();
        let input = Tensor::from_array((shape.to_vec(), random(len, 7))).unwrap();
        let outputs = session.run(ort::inputs!["x" => input]).unwrap();
        let (_, data) = outputs["y"].try_extract_tensor::<f32>().unwrap();
        data.to_vec()
    }

    /// 改寫前後在 ORT CPU 上逐元素相同。
    fn assert_equivalent(original: ModelProto, shape: &[usize]) -> RewriteStats {
        let expected = run(&original, shape);
        let mut rewritten = original;
        let stats = rewrite_for_coreml(&mut rewritten);
        let actual = run(&rewritten, shape);
        assert_eq!(expected.len(), actual.len(), "輸出長度不同");
        for (i, (a, b)) in expected.iter().zip(&actual).enumerate() {
            assert!((a - b).abs() <= 1e-5, "第 {i} 個元素不同：{a} vs {b}");
        }
        stats
    }

    fn attr_node(mut node: NodeProto, attributes: Vec<AttributeProto>) -> NodeProto {
        node.attribute = attributes;
        node
    }

    fn string_attribute(name: &str, value: &str) -> AttributeProto {
        let mut attribute = AttributeProto::new();
        attribute.name = name.into();
        attribute.type_ = EnumOrUnknown::new(AttributeType::STRING);
        attribute.s = value.as_bytes().to_vec();
        attribute
    }

    fn int_attribute(name: &str, value: i64) -> AttributeProto {
        let mut attribute = AttributeProto::new();
        attribute.name = name.into();
        attribute.type_ = EnumOrUnknown::new(AttributeType::INT);
        attribute.i = value;
        attribute
    }

    fn scalar_index(name: &str, value: i64) -> TensorProto {
        let mut tensor = TensorProto::new();
        tensor.name = name.into();
        tensor.data_type = DataType::INT64 as i32;
        tensor.int64_data = vec![value];
        tensor
    }

    #[test]
    fn einsum_matrix_product_becomes_matmul() {
        let node = attr_node(
            make_node(
                "Einsum",
                "e",
                vec!["x".into(), "w".into()],
                vec!["y".into()],
            ),
            vec![string_attribute("equation", "ij,jk->ik")],
        );
        let m = model(
            vec![node],
            vec![float_tensor("w", &[4, 5], 3)],
            &[3, 4],
            &[3, 5],
        );
        let stats = assert_equivalent(m, &[3, 4]);
        assert_eq!(stats.einsum, 1);
    }

    #[test]
    fn other_einsum_equations_are_left_alone() {
        let node = attr_node(
            make_node(
                "Einsum",
                "e",
                vec!["x".into(), "w".into()],
                vec!["y".into()],
            ),
            vec![string_attribute("equation", "ij,kj->ik")],
        );
        let mut m = model(
            vec![node],
            vec![float_tensor("w", &[5, 4], 3)],
            &[3, 4],
            &[3, 5],
        );
        assert_eq!(rewrite_for_coreml(&mut m), RewriteStats::default());
        assert_eq!(m.graph.node[0].op_type, "Einsum");
    }

    #[test]
    fn neg_becomes_multiply_by_minus_one() {
        let node = make_node("Neg", "n", vec!["x".into()], vec!["y".into()]);
        let stats = assert_equivalent(model(vec![node], vec![], &[2, 3], &[2, 3]), &[2, 3]);
        assert_eq!(stats.neg, 1);
    }

    #[test]
    fn scalar_gather_on_rank_five_becomes_slice_and_squeeze() {
        for (axis, index) in [(1, 1), (4, -1), (2, -2), (0, 0)] {
            let node = attr_node(
                make_node(
                    "Gather",
                    "g",
                    vec!["x".into(), "i".into()],
                    vec!["y".into()],
                ),
                vec![int_attribute("axis", axis)],
            );
            let shape = [2usize, 3, 4, 2, 5];
            let mut out: Vec<i64> = shape.iter().map(|&d| d as i64).collect();
            out.remove(axis as usize);
            let m = model(
                vec![node],
                vec![scalar_index("i", index)],
                &shape.map(|d| d as i64),
                &out,
            );
            let stats = assert_equivalent(m, &shape);
            assert_eq!(stats.gather, 1, "axis {axis} index {index}");
        }
    }

    #[test]
    fn gather_with_constant_node_index_is_rewritten() {
        let mut constant = make_node("Constant", "c", vec![], vec!["i".into()]);
        let mut value = AttributeProto::new();
        value.name = "value".into();
        value.type_ = EnumOrUnknown::new(AttributeType::TENSOR);
        value.t = MessageField::some(scalar_index("", 2));
        constant.attribute.push(value);
        let gather = attr_node(
            make_node(
                "Gather",
                "g",
                vec!["x".into(), "i".into()],
                vec!["y".into()],
            ),
            vec![int_attribute("axis", 1)],
        );
        let stats = assert_equivalent(
            model(vec![constant, gather], vec![], &[2, 4], &[2]),
            &[2, 4],
        );
        assert_eq!(stats.gather, 1);
    }

    #[test]
    fn gather_with_vector_index_is_left_alone() {
        let mut index = scalar_index("i", 1);
        index.dims = vec![1];
        let node = make_node(
            "Gather",
            "g",
            vec!["x".into(), "i".into()],
            vec!["y".into()],
        );
        let mut m = model(vec![node], vec![index], &[3, 4], &[1, 4]);
        assert_eq!(rewrite_for_coreml(&mut m).gather, 0);
        assert_eq!(m.graph.node[0].op_type, "Gather");
    }

    fn conv_transpose(pads: &[i64], output_padding: &[i64]) -> NodeProto {
        let mut attributes = vec![
            ints_attr("kernel_shape", &[3, 3]),
            ints_attr("strides", &[2, 2]),
            ints_attr("pads", pads),
        ];
        if !output_padding.is_empty() {
            attributes.push(ints_attr("output_padding", output_padding));
        }
        attr_node(
            make_node(
                "ConvTranspose",
                "ct",
                vec!["x".into(), "w".into()],
                vec!["y".into()],
            ),
            attributes,
        )
    }

    #[test]
    fn conv_transpose_output_padding_becomes_full_output_and_crop() {
        // LaMa 的上取樣：k3 s2 p1 op1，輸出剛好是輸入的兩倍
        let node = conv_transpose(&[1, 1, 1, 1], &[1, 1]);
        let w = float_tensor("w", &[2, 3, 3, 3], 5);
        let m = model(vec![node], vec![w], &[1, 2, 5, 6], &[1, 3, 10, 12]);
        let stats = assert_equivalent(m.clone(), &[1, 2, 5, 6]);
        assert_eq!(stats.conv_transpose, 1);
        let mut rewritten = m;
        rewrite_for_coreml(&mut rewritten);
        let ops: Vec<_> = rewritten
            .graph
            .node
            .iter()
            .map(|n| n.op_type.as_str())
            .collect();
        assert_eq!(ops, ["ConvTranspose", "Slice"]);
        assert!(
            rewritten.graph.node[0]
                .attribute
                .iter()
                .all(|a| a.name != "output_padding")
        );
    }

    #[test]
    fn conv_transpose_output_padding_that_balances_pads_needs_no_crop() {
        let node = conv_transpose(&[1, 1, 2, 2], &[1, 1]);
        let w = float_tensor("w", &[2, 3, 3, 3], 5);
        let m = model(vec![node], vec![w], &[1, 2, 5, 6], &[1, 3, 9, 11]);
        let stats = assert_equivalent(m.clone(), &[1, 2, 5, 6]);
        assert_eq!(stats.conv_transpose, 1);
        let mut rewritten = m;
        rewrite_for_coreml(&mut rewritten);
        assert_eq!(rewritten.graph.node.len(), 1);
    }

    #[test]
    fn plain_conv_transpose_is_left_alone() {
        let node = conv_transpose(&[1, 1, 1, 1], &[]);
        let mut m = model(
            vec![node],
            vec![float_tensor("w", &[2, 3, 3, 3], 5)],
            &[1, 2, 5, 6],
            &[1, 3, 9, 11],
        );
        assert_eq!(rewrite_for_coreml(&mut m), RewriteStats::default());
    }

    #[test]
    fn batch_dimension_is_fixed() {
        let mut m = model(vec![], vec![], &[-1, 3], &[-1, 3]);
        fix_dim_param(&mut m, "batch", 1);
        let dim = &m.graph.input[0].type_.tensor_type().shape.dim[0];
        assert!(!dim.has_dim_param());
        assert_eq!(dim.dim_value(), 1);
    }

    #[test]
    fn converted_files_carry_the_converter_version() {
        let original = Path::new("/cache/n2p/lama_fp32.onnx");
        assert_eq!(
            converted_path(original),
            PathBuf::from(format!(
                "/cache/n2p/lama_fp32.coreml-v{CONVERTER_VERSION}.onnx"
            ))
        );
        assert_eq!(
            compile_cache_dir(original),
            PathBuf::from(format!("/cache/n2p/coreml/v{CONVERTER_VERSION}"))
        );
    }
}
