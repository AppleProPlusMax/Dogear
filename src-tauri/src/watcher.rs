use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::AddClipboardFormatListener;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    TranslateMessage, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLIPBOARDUPDATE,
    WNDCLASSW,
};

use crate::clipboard::{self, ClipboardRead};
use crate::detect::Detection;
use crate::state::{self, AppState, Clip};

pub const HISTORY_LIMIT: usize = 200;
/// 连着复制时先在后台记完，界面大约每 50 毫秒合并刷新一次。
const FLUSH_AFTER: Duration = Duration::from_millis(50);

static APP: OnceLock<AppHandle> = OnceLock::new();
static CHANGES: OnceLock<Sender<ClipStep>> = OnceLock::new();

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipStep {
    clip: Clip,
    removed_ids: Vec<i64>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipsChanged {
    steps: Vec<ClipStep>,
}

pub fn start(app: AppHandle) {
    let (tx, rx) = mpsc::channel();
    let _ = CHANGES.set(tx);
    let emitter = app.clone();
    std::thread::spawn(move || flush_loop(emitter, rx));
    let _ = APP.set(app);
    std::thread::spawn(|| {
        if let Err(err) = unsafe { listen_loop() } {
            eprintln!("剪切板监听启动失败: {err}");
        }
    });
}

/// 记录马上做完。界面通知攒一小段时间，避免每来一条就把窗口重画一遍。
fn flush_loop(app: AppHandle, rx: Receiver<ClipStep>) {
    while let Ok(first) = rx.recv() {
        let mut steps = vec![first];
        let deadline = Instant::now() + FLUSH_AFTER;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match rx.recv_timeout(remaining) {
                Ok(step) => steps.push(step),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    publish(&app, steps);
                    return;
                }
            }
        }
        publish(&app, steps);
    }
}

/// 发出去之前再读一次当前记录。识别可能在这几十毫秒里已经做完，不能把刚入库时的「正在识别」发出去。
fn publish(app: &AppHandle, mut steps: Vec<ClipStep>) {
    if let Some(state) = app.try_state::<Mutex<AppState>>() {
        if let Ok(guard) = state.lock() {
            for step in &mut steps {
                if let Some(current) = guard.clips.iter().find(|clip| clip.id == step.clip.id) {
                    step.clip = current.clone();
                }
            }
        }
    }
    let _ = app.emit("clips-changed", &ClipsChanged { steps });
}

fn notify(step: ClipStep) {
    let Some(tx) = CHANGES.get() else {
        return;
    };
    let _ = tx.send(step);
}

unsafe fn listen_loop() -> Result<(), String> {
    let instance = GetModuleHandleW(None).map_err(|err| err.to_string())?;
    let class_name = w!("DogearClipboardListener");
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: instance.into(),
        lpszClassName: class_name,
        ..Default::default()
    };
    RegisterClassW(&window_class);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class_name,
        w!(""),
        WINDOW_STYLE::default(),
        0,
        0,
        0,
        0,
        Some(HWND_MESSAGE),
        None,
        Some(instance.into()),
        None,
    )
    .map_err(|err| err.to_string())?;
    AddClipboardFormatListener(hwnd).map_err(|err| err.to_string())?;

    let mut message = MSG::default();
    while GetMessageW(&mut message, None, 0, 0).0 != 0 {
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    Ok(())
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        if let Some(app) = APP.get() {
            on_clipboard(app);
        }
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn on_clipboard(app: &AppHandle) {
    if recording_paused(app) {
        return;
    }
    match clipboard::read() {
        ClipboardRead::Text(text) => record_text(app, text),
        ClipboardRead::Image(image) => record_image(app, &image.png, &image.hash),
        ClipboardRead::Ignored => {
            eprintln!("已跳过一条剪切板内容（敏感标记或超出长度上限），未记录正文");
        }
        ClipboardRead::Empty => {}
    }
}

pub(crate) fn record_image(app: &AppHandle, png: &[u8], hash: &str) {
    if clipboard::is_self_image(hash) {
        return;
    }
    let Some(state) = app.try_state::<Mutex<AppState>>() else {
        return;
    };
    let now = state::now_ms();

    // 同一张图再复制一次：挪到最前，识别结果直接复用。
    {
        let Ok(mut guard) = state.lock() else {
            return;
        };
        if let Some(existing) = guard.clips.iter_mut().find(|clip| {
            clip.kind == "image" && clip.hash.as_deref() == Some(hash)
        }) {
            existing.created_at = now;
            let updated = existing.clone();
            let id = updated.id;
            guard.clips.retain(|clip| clip.id != id);
            guard.clips.insert(0, updated.clone());
            drop(guard);
            crate::storage::touch_clip(id, now);
            eprintln!("剪切板图片已存在，挪到最前");
            notify(ClipStep {
                clip: updated,
                removed_ids: Vec::new(),
            });
            return;
        }
    }

    let saved = match crate::images::save(app, png, hash) {
        Ok(saved) => saved,
        Err(err) => {
            eprintln!("剪切板图片落盘失败: {err}");
            return;
        }
    };

    let Ok(mut guard) = state.lock() else {
        return;
    };
    let id = guard.next_id;
    let ocr_auto = guard.settings.ocr_auto;
    guard.next_id += 1;
    guard.clips.insert(
        0,
        Clip {
            id,
            content: String::new(),
            created_at: now,
            kind: "image".into(),
            language: None,
            file_path: Some(saved.path.to_string_lossy().into_owned()),
            thumb_path: Some(saved.thumb_path.to_string_lossy().into_owned()),
            hash: Some(hash.to_string()),
            width: Some(saved.width),
            height: Some(saved.height),
            ocr_status: if ocr_auto { "pending" } else { "none" }.into(),
            ocr_text: None,
            ocr_lang: None,
        },
    );
    let (removed_ids, evicted_hashes) = trim_history(&mut guard);
    let clip = guard.clips.first().cloned();
    drop(guard);

    eprintln!("已记录剪切板图片，{}×{}", saved.width, saved.height);
    if let Some(clip) = clip {
        crate::storage::save_step(&clip, &removed_ids);
        crate::images::remove_hashes(app, &evicted_hashes);
        notify(ClipStep { clip, removed_ids });
    }
    if ocr_auto {
        crate::ocr::enqueue(id);
    }
}

fn record_text(app: &AppHandle, text: String) {
    if clipboard::is_self_write(&text) {
        return;
    }
    let Some(state) = app.try_state::<Mutex<AppState>>() else {
        return;
    };
    // 哈希和类型判断放在拿锁之前，避免窗口线程为了读列表干等。
    let digest = crate::images::hash(text.as_bytes());
    let detected = crate::detect::classify(&text);
    let now = state::now_ms();
    let Ok(mut guard) = state.lock() else {
        return;
    };
    let (step, evicted_hashes) = insert_text(&mut guard, text, now, digest, detected);
    let chars = step.clip.content.chars().count();
    let kind = step.clip.kind.clone();
    drop(guard);
    crate::storage::save_step(&step.clip, &step.removed_ids);
    crate::images::remove_hashes(app, &evicted_hashes);
    eprintln!("已记录剪切板文本，长度 {chars} 字，类型 {kind}");
    notify(step);
}

fn insert_text(
    guard: &mut AppState,
    text: String,
    now: i64,
    digest: String,
    detected: Detection,
) -> (ClipStep, Vec<String>) {
    if let Some(index) = guard.clips.iter().position(|clip| {
        clip.kind != "image" && clip.hash.as_deref() == Some(digest.as_str()) && clip.content == text
    }) {
        let mut existing = guard.clips.remove(index);
        existing.created_at = now;
        let clip = existing.clone();
        guard.clips.insert(0, existing);
        return (
            ClipStep {
                clip,
                removed_ids: Vec::new(),
            },
            Vec::new(),
        );
    }
    let mut clip = Clip::text(
        guard.next_id,
        text,
        now,
        detected.kind.to_string(),
        detected.language.map(str::to_string),
    );
    clip.hash = Some(digest);
    guard.next_id += 1;
    guard.clips.insert(0, clip.clone());
    let (removed_ids, evicted_hashes) = trim_history(guard);
    (ClipStep { clip, removed_ids }, evicted_hashes)
}

fn recording_paused(app: &AppHandle) -> bool {
    app.try_state::<Mutex<AppState>>()
        .and_then(|state| state.lock().ok().map(|guard| guard.settings.pause_recording))
        .unwrap_or(false)
}

fn trim_history(guard: &mut AppState) -> (Vec<i64>, Vec<String>) {
    if guard.clips.len() <= HISTORY_LIMIT {
        return (Vec::new(), Vec::new());
    }
    let removed = guard.clips.split_off(HISTORY_LIMIT);
    let mut removed_ids = Vec::with_capacity(removed.len());
    let mut evicted_hashes = Vec::new();
    for clip in removed {
        removed_ids.push(clip.id);
        if clip.kind == "image" {
            if let Some(hash) = clip.hash {
                let still_used = guard
                    .clips
                    .iter()
                    .any(|item| item.kind == "image" && item.hash.as_deref() == Some(hash.as_str()));
                if !still_used {
                    evicted_hashes.push(hash);
                }
            }
        }
    }
    (removed_ids, evicted_hashes)
}

#[cfg(test)]
mod tests {
    use super::{insert_text, trim_history, HISTORY_LIMIT};
    use crate::detect::classify;
    use crate::images::hash;
    use crate::state::{AppState, Clip};

    fn store(guard: &mut AppState, text: &str, now: i64) -> i64 {
        let (step, _) = insert_text(
            guard,
            text.to_string(),
            now,
            hash(text.as_bytes()),
            classify(text),
        );
        step.clip.id
    }

    #[test]
    fn duplicate_text_moves_to_front_and_keeps_id() {
        let mut guard = AppState::default();
        let first = store(&mut guard, "alpha", 1);
        let second = store(&mut guard, "beta", 2);
        let again = store(&mut guard, "alpha", 3);
        assert_eq!(again, first);
        assert_eq!(guard.clips[0].id, first);
        assert_eq!(guard.clips[0].created_at, 3);
        assert_eq!(guard.clips[1].id, second);
        assert_eq!(guard.clips.len(), 2);
    }

    #[test]
    fn history_drops_the_oldest_past_the_limit() {
        let mut guard = AppState::default();
        let mut oldest = 0;
        for index in 0..HISTORY_LIMIT {
            let id = store(&mut guard, &format!("item-{index}"), index as i64);
            if index == 0 {
                oldest = id;
            }
        }
        let removed = {
            let digest = hash(b"newer");
            let (step, _) = insert_text(&mut guard, "newer".into(), 999, digest, classify("newer"));
            step.removed_ids
        };
        assert_eq!(removed, vec![oldest]);
        assert_eq!(guard.clips.len(), HISTORY_LIMIT);
        assert_eq!(guard.clips[0].content, "newer");
        assert!(guard.clips.iter().all(|clip: &Clip| clip.id != oldest));
        assert!(trim_history(&mut guard).0.is_empty());
    }

    #[test]
    fn evicting_image_reports_its_hash_for_cleanup() {
        let mut guard = AppState::default();
        guard.clips.push(Clip {
            id: 1,
            content: String::new(),
            created_at: 1,
            kind: "image".into(),
            language: None,
            file_path: Some("old.png".into()),
            thumb_path: Some("old-thumb.png".into()),
            hash: Some("old-img-hash".into()),
            width: Some(10),
            height: Some(10),
            ocr_status: "none".into(),
            ocr_text: None,
            ocr_lang: None,
        });
        guard.next_id = 2;
        for index in 1..HISTORY_LIMIT {
            store(&mut guard, &format!("item-{index}"), index as i64);
        }
        let digest = hash(b"overflow");
        let (step, evicted_hashes) =
            insert_text(&mut guard, "overflow".into(), 999, digest, classify("overflow"));
        assert_eq!(step.removed_ids, vec![1]);
        assert_eq!(evicted_hashes, vec!["old-img-hash".to_string()]);
    }
}
