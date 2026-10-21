//! 本機模型清單：命令列（`n2p models`）與 GUI 共用。

use std::path::{Path, PathBuf};

/// 模型的用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// LaMa：去字修補。
    Erase,
    /// SAM：物件切出。
    Objects,
}

/// 一個模型檔。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFile {
    pub name: &'static str,
    pub purpose: Purpose,
    pub path: PathBuf,
    pub url: &'static str,
    pub expected_size: u64,
}

/// 模型檔的狀態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelState {
    Missing,
    /// 下載到一半（或被截斷）。
    Partial {
        size: u64,
    },
    Ready,
}

impl ModelFile {
    pub fn state(&self) -> ModelState {
        match std::fs::metadata(&self.path) {
            Err(_) => ModelState::Missing,
            Ok(meta) if meta.len() >= self.expected_size => ModelState::Ready,
            Ok(meta) => ModelState::Partial { size: meta.len() },
        }
    }

    /// LaMa 的 GPU 版是否已轉換好。其他模型或非 macOS 時為 `None`（不適用）。
    pub fn gpu_converted(&self) -> Option<bool> {
        (self.purpose == Purpose::Erase && cfg!(target_os = "macos"))
            .then(|| crate::gpu::converted_path(&self.path).exists())
    }

    /// 用系統 curl 下載（可續傳、不必為此引入 HTTP 依賴）。已就緒時直接回傳。
    /// `progress_bar` 為 `true` 時讓 curl 把進度條印到終端機。
    pub fn fetch(&self, progress_bar: bool) -> Result<(), String> {
        if self.state() == ModelState::Ready {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        // 寫死路徑：從 Finder 啟動的 app 只有最小的 PATH
        let mut command = std::process::Command::new("/usr/bin/curl");
        command.args(["-L", "--fail", "-C", "-", "-o"]);
        command.arg(&self.path).arg(self.url);
        command.arg(if progress_bar {
            "--progress-bar"
        } else {
            "--silent"
        });
        let status = command
            .status()
            .map_err(|e| format!("無法執行 curl：{e}"))?;
        if !status.success() {
            return Err(format!("下載失敗（curl 結束碼 {:?}）", status.code()));
        }
        match self.state() {
            ModelState::Ready => Ok(()),
            ModelState::Partial { size } => Err(format!(
                "下載不完整（{size} / {} 位元組），請重試",
                self.expected_size
            )),
            ModelState::Missing => Err("下載後找不到檔案".into()),
        }
    }
}

/// 全部模型（LaMa、SAM encoder、SAM decoder）。無法判斷快取目錄時回傳錯誤。
pub fn manifest() -> Result<Vec<ModelFile>, String> {
    let lama_path =
        crate::lama::default_model_path().ok_or("無法判斷快取目錄（請設定 N2P_LAMA_MODEL）")?;
    let sam_dir =
        crate::sam::default_model_dir().ok_or("無法判斷快取目錄（請設定 N2P_SAM_DIR）")?;
    Ok(manifest_at(&lama_path, &sam_dir))
}

fn manifest_at(lama_path: &Path, sam_dir: &Path) -> Vec<ModelFile> {
    vec![
        ModelFile {
            name: "LaMa 模型（去字修補，約 199 MiB）",
            purpose: Purpose::Erase,
            path: lama_path.to_path_buf(),
            url: crate::lama::MODEL_URL,
            expected_size: crate::lama::MODEL_SIZE,
        },
        ModelFile {
            name: "SAM encoder（物件切出，約 27 MiB）",
            purpose: Purpose::Objects,
            path: sam_dir.join(crate::sam::ENCODER_FILE),
            url: crate::sam::ENCODER_URL,
            expected_size: crate::sam::ENCODER_SIZE,
        },
        ModelFile {
            name: "SAM decoder（物件切出，約 16 MiB）",
            purpose: Purpose::Objects,
            path: sam_dir.join(crate::sam::DECODER_FILE),
            url: crate::sam::DECODER_URL,
            expected_size: crate::sam::DECODER_SIZE,
        },
    ]
}

/// 已下載與應下載的位元組數（用來顯示下載進度）。
pub fn download_progress(models: &[ModelFile]) -> (u64, u64) {
    models.iter().fold((0, 0), |(done, total), model| {
        let size = match model.state() {
            ModelState::Ready => model.expected_size,
            ModelState::Partial { size } => size.min(model.expected_size),
            ModelState::Missing => 0,
        };
        (done + size, total + model.expected_size)
    })
}

/// 預先產生 LaMa 的 GPU 版（macOS；約 1 秒）。LaMa 尚未下載或非 macOS 時不做事。
///
/// 第一次載入時也會自動轉換，這裡只是讓下載完成後的第一份工作不必等。
pub fn prepare_gpu(models: &[ModelFile]) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    for model in models {
        if model.purpose == Purpose::Erase && model.state() == ModelState::Ready {
            crate::gpu::ensure_converted(&model.path)?;
        }
    }
    Ok(())
}

/// 某個用途的模型是否全部就緒。
pub fn ready(models: &[ModelFile], purpose: Purpose) -> bool {
    models
        .iter()
        .filter(|m| m.purpose == purpose)
        .all(|m| m.state() == ModelState::Ready)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_follow_file_size() {
        let dir = std::env::temp_dir().join("n2p-models-state-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let models = manifest_at(&dir.join("lama.onnx"), &dir);
        assert_eq!(models.len(), 3);
        assert!(models.iter().all(|m| m.state() == ModelState::Missing));
        assert!(!ready(&models, Purpose::Objects));

        std::fs::write(&models[1].path, b"short").unwrap();
        assert_eq!(models[1].state(), ModelState::Partial { size: 5 });

        for model in &models[1..] {
            let file = std::fs::File::create(&model.path).unwrap();
            file.set_len(model.expected_size).unwrap();
        }
        assert!(ready(&models, Purpose::Objects));
        assert!(!ready(&models, Purpose::Erase));
        let sam_bytes = models[1].expected_size + models[2].expected_size;
        assert_eq!(
            download_progress(&models),
            (sam_bytes, sam_bytes + models[0].expected_size)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
