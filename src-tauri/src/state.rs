use serde::Serialize;

use crate::effects::EffectState;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: i64,
    pub content: String,
    pub created_at: i64,
    pub kind: String,
    pub language: Option<String>,
}

pub struct AppState {
    pub clips: Vec<Clip>,
    pub next_id: i64,
    pub previous_hwnd: isize,
    pub ignore_blur_until: i64,
    /// 顶部空白处开始拖动后的一小段时间。这段里的失焦来自系统拖动，不是点到了外面。
    pub drag_blur_until: i64,
    pub effect: EffectState,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            clips: Vec::new(),
            next_id: 1,
            previous_hwnd: 0,
            ignore_blur_until: 0,
            drag_blur_until: 0,
            effect: EffectState::unknown(),
        }
    }
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
