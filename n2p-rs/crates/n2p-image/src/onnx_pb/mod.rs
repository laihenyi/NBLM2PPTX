//! ONNX protobuf 型別。
//!
//! `onnx.rs` 由 rust-protobuf 3.7.2 從 `proto/onnx.proto`（ONNX 官方，Apache-2.0）產生，
//! 放進版控以免建置時需要 codegen。重新產生：
//!
//! ```text
//! protobuf_codegen::Codegen::new().pure().includes(["proto"]).input("proto/onnx.proto")
//!     .out_dir("src/onnx_pb").run_from_script();
//! ```

#[allow(clippy::all, clippy::pedantic, unused, missing_docs)]
pub mod onnx;
