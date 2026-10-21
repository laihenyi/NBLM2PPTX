//! 行程共用的 ONNX Runtime 環境。
//!
//! `ort` 每個行程只允許建立一次環境：第二次 `ort::init().build()` 一律回錯
//! （"only one environment is allowed per process"），即使第一個環境已經釋放。
//! LaMa 與 SAM 都要載入模型，所以環境必須集中建立、共用。

use std::sync::OnceLock;

use ort::environment::Environment;

static ENVIRONMENT: OnceLock<Result<Environment, String>> = OnceLock::new();

/// 取得共用環境（第一次呼叫時建立；建立失敗會永久回傳同一個錯誤）。
pub fn environment() -> Result<&'static Environment, String> {
    ENVIRONMENT
        .get_or_init(|| {
            ort::init()
                .with_name("n2p")
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}
