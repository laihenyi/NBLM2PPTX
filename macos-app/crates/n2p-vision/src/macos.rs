//! macOS Vision framework 的實際呼叫。
//!
//! 走 `VNImageRequestHandler initWithData:options:` —— 直接餵 PNG bytes，
//! 省掉 CGImage 的整條管線（也因此不需要 objc2-core-graphics）。

use objc2::AllocAnyThread;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2_foundation::{NSArray, NSData, NSDictionary, NSError, NSString};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation, VNRequest,
    VNRequestTextRecognitionLevel,
};

use crate::{Error, Observation, Result, VisionConfig};

/// 讀取 macOS 系統偏好語言（`NSLocale.preferredLanguages`）。
pub fn preferred_languages() -> Vec<String> {
    let Some(cls) = AnyClass::get(c"NSLocale") else {
        return Vec::new();
    };
    let langs: Retained<NSArray<NSString>> = unsafe { msg_send![cls, preferredLanguages] };
    (0..langs.count())
        .map(|i| langs.objectAtIndex(i).to_string())
        .collect()
}

pub fn recognize_png(png: &[u8], config: &VisionConfig) -> Result<Vec<Observation>> {
    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(if config.accurate {
        VNRequestTextRecognitionLevel::Accurate
    } else {
        VNRequestTextRecognitionLevel::Fast
    });
    request.setUsesLanguageCorrection(config.language_correction);
    request.setMinimumTextHeight(config.minimum_text_height);

    let languages: Vec<Retained<NSString>> = config
        .languages
        .iter()
        .map(|language| NSString::from_str(language))
        .collect();
    if !languages.is_empty() {
        request.setRecognitionLanguages(&NSArray::from_retained_slice(&languages));
    }

    let data = NSData::with_bytes(png);
    let options = NSDictionary::new();
    // SAFETY: `initWithData:options:` 只讀取我們持有的 data 與 options，且兩者存活到呼叫結束。
    let handler = VNImageRequestHandler::initWithData_options(
        VNImageRequestHandler::alloc(),
        &data,
        &options,
    );

    let requests: Retained<NSArray<VNRequest>> = {
        let upstream: &VNRequest = &request;
        NSArray::from_slice(&[upstream])
    };

    handler
        .performRequests_error(&requests)
        .map_err(|error: Retained<NSError>| Error::Vision(error.to_string()))?;

    let results = request
        .results()
        .ok_or_else(|| Error::Vision("Vision 沒有回傳結果".into()))?;

    let mut observations = Vec::with_capacity(results.len());
    for index in 0..results.len() {
        let observation: Retained<VNRecognizedTextObservation> = results.objectAtIndex(index);
        let candidates = observation.topCandidates(1);
        if candidates.is_empty() {
            continue;
        }
        let best = candidates.objectAtIndex(0);
        let text = best.string().to_string();
        let confidence = best.confidence();
        let rect = unsafe { observation.boundingBox() };

        // Vision 的原點在左下、y 向上；轉成左上、y 向下。
        let x = rect.origin.x;
        let width = rect.size.width;
        let height = rect.size.height;
        let y_top = 1.0 - (rect.origin.y + height);

        observations.push(Observation {
            text,
            confidence,
            bbox_norm: (x, y_top, width, height),
        });
    }

    Ok(observations)
}
