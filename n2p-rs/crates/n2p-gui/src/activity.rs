//! 轉換期間告訴 macOS「使用者正在等這件事」。
//!
//! 視窗不在前景時，App Nap 會降低整個行程的優先權：實測轉換慢了約 3 倍。
//! `NSActivityUserInitiated` 關掉 App Nap，並在轉換期間防止系統閒置睡眠。

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};

/// 存活期間維持活動；離開作用域時結束。
pub struct Activity(Retained<ProtocolObject<dyn NSObjectProtocol>>);

impl Activity {
    pub fn begin(reason: &str) -> Self {
        let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
            NSActivityOptions::UserInitiated,
            &NSString::from_str(reason),
        );
        Self(token)
    }
}

impl Drop for Activity {
    fn drop(&mut self) {
        // SAFETY: 這個物件就是 beginActivityWithOptions:reason: 回傳的那一個
        unsafe { NSProcessInfo::processInfo().endActivity(&self.0) };
    }
}
