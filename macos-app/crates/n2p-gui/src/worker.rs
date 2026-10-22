//! 背景執行緒：依序執行工作，把進度送回介面。
//!
//! 一次只跑一個工作 —— LaMa 與 SAM 都很吃記憶體，平行執行只會互搶。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use n2p_pipeline::{ConvertOptions, ConvertReport, Progress};

use crate::i18n::Language;
use crate::model::{Job, partial_path};

/// 背景執行緒送回介面的事件。`index` 是工作在佇列中的位置。
#[derive(Debug)]
pub enum Event {
    Started {
        index: usize,
    },
    Progress {
        index: usize,
        progress: Progress,
    },
    Finished {
        index: usize,
        result: Outcome,
    },
    /// 所有工作結束（含取消）。
    Idle,
    /// 模型下載結果。
    Fetched(Result<(), String>),
}

#[derive(Debug)]
pub enum Outcome {
    Done(ConvertReport),
    Failed(String),
    Cancelled,
}

/// 依序執行 `jobs`。每送出一個事件就呼叫 `wake`，讓介面立即重畫。
pub fn spawn(
    jobs: Vec<(usize, Job, ConvertOptions)>,
    cancel: Arc<AtomicBool>,
    events: Sender<Event>,
    lang: Language,
    wake: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        let send = |event: Event| {
            let _ = events.send(event);
            wake();
        };
        let _activity = crate::activity::Activity::begin(lang.strings().activity_converting);
        for (index, job, options) in jobs {
            if cancel.load(Ordering::Relaxed) {
                send(Event::Finished {
                    index,
                    result: Outcome::Cancelled,
                });
                continue;
            }
            send(Event::Started { index });
            let result = run(&job, &options, &cancel, lang, &mut |progress| {
                send(Event::Progress { index, progress })
            });
            // 模型與推論暫存會佔住數 GB，閒置時不該留著
            n2p_pipeline::release_models();
            send(Event::Finished { index, result });
        }
        send(Event::Idle);
    });
}

fn run(
    job: &Job,
    options: &ConvertOptions,
    cancel: &AtomicBool,
    lang: Language,
    report_progress: &mut dyn FnMut(Progress),
) -> Outcome {
    // 先寫到暫存檔，成功才改名：失敗或取消都不會毀掉既有的輸出檔
    let partial = partial_path(&job.out);
    let result =
        n2p_pipeline::convert_with_progress(&job.pdf, &partial, options, &mut |progress| {
            report_progress(progress);
            !cancel.load(Ordering::Relaxed)
        });
    match result {
        Ok(report) => match std::fs::rename(&partial, &job.out) {
            Ok(()) => Outcome::Done(report),
            Err(error) => {
                let _ = std::fs::remove_file(&partial);
                Outcome::Failed(lang.write_error(&job.out, &error))
            }
        },
        Err(n2p_pipeline::Error::Cancelled) => {
            let _ = std::fs::remove_file(&partial);
            Outcome::Cancelled
        }
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            Outcome::Failed(error.to_string())
        }
    }
}

/// 在背景下載所有缺少的模型。
pub fn fetch_models(events: Sender<Event>, lang: Language, wake: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        let _activity = crate::activity::Activity::begin(lang.strings().activity_downloading);
        let result = n2p_image::models::manifest().and_then(|models| {
            models.iter().try_for_each(|model| {
                model
                    .fetch(false)
                    .map_err(|e| format!("{}：{e}", model.name))
            })?;
            // 轉換失敗不算下載失敗：第一次載入時會再試，不行就用 CPU
            let _ = n2p_image::models::prepare_gpu(&models);
            Ok(())
        });
        let _ = events.send(Event::Fetched(result));
        wake();
    });
}
