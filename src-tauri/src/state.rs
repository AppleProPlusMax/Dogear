use serde::Serialize;

use crate::effects::EffectState;
use crate::settings::Settings;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: i64,
    pub content: String,
    pub created_at: i64,
    pub kind: String,
    pub language: Option<String>,
    /// 图片原图的本地路径，文本条目为空。
    #[serde(skip)]
    pub file_path: Option<String>,
    #[serde(skip)]
    pub thumb_path: Option<String>,
    /// 图片内容哈希，用于去重。
    #[serde(skip)]
    pub hash: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// none / pending / done / empty / failed
    pub ocr_status: String,
    pub ocr_text: Option<String>,
    pub ocr_lang: Option<String>,
}

impl Clip {
    pub fn text(id: i64, content: String, created_at: i64, kind: String, language: Option<String>) -> Self {
        Self {
            id,
            content,
            created_at,
            kind,
            language,
            file_path: None,
            thumb_path: None,
            hash: None,
            width: None,
            height: None,
            ocr_status: "none".into(),
            ocr_text: None,
            ocr_lang: None,
        }
    }
}

pub struct AppState {
    pub clips: Vec<Clip>,
    pub next_id: i64,
    pub previous_hwnd: isize,
    pub ignore_blur_until: i64,
    /// 顶部空白处开始拖动后的一小段时间。这段里的失焦来自系统拖动，不是点到了外面。
    pub drag_blur_until: i64,
    pub effect: EffectState,
    pub settings: Settings,
    /// 正在录制新快捷键。这段时间里用户快捷键先卸掉，避免按下时把窗口关掉。
    pub shortcut_capture: bool,
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
            settings: Settings::default(),
            shortcut_capture: false,
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
