use std::sync::Mutex;

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
use crate::state::{self, AppState, Clip};

static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

pub fn start(app: AppHandle) {
    let _ = APP.set(app);
    std::thread::spawn(|| {
        if let Err(err) = unsafe { listen_loop() } {
            eprintln!("剪切板监听启动失败: {err}");
        }
    });
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
        ClipboardRead::Text(text) => {
            if clipboard::is_self_write(&text) {
                return;
            }
            let Some(state) = app.try_state::<Mutex<AppState>>() else {
                return;
            };
            let Ok(mut guard) = state.lock() else {
                return;
            };
            let now = state::now_ms();
            if let Some(existing) = guard.clips.iter_mut().find(|clip| clip.content == text) {
                existing.created_at = now;
                let updated = existing.clone();
                let id = updated.id;
                guard.clips.retain(|clip| clip.id != id);
                guard.clips.insert(0, updated);
            } else {
                let detected = crate::detect::classify(&text);
                let clip = Clip::text(
                    guard.next_id,
                    text,
                    now,
                    detected.kind.to_string(),
                    detected.language.map(str::to_string),
                );
                guard.next_id += 1;
                guard.clips.insert(0, clip);
                truncate(&mut guard);
            }
            let (chars, kind) = guard
                .clips
                .first()
                .map(|clip| (clip.content.chars().count(), clip.kind.clone()))
                .unwrap_or((0, "text".into()));
            drop(guard);
            eprintln!("已记录剪切板文本，长度 {chars} 字，类型 {kind}");
            let _ = app.emit("clip-added", ());
        }
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
        if let Some(existing) = guard
            .clips
            .iter_mut()
            .find(|clip| clip.hash.as_deref() == Some(hash))
        {
            existing.created_at = now;
            let updated = existing.clone();
            let id = updated.id;
            guard.clips.retain(|clip| clip.id != id);
            guard.clips.insert(0, updated);
            drop(guard);
            eprintln!("剪切板图片已存在，挪到最前");
            let _ = app.emit("clip-added", ());
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
    truncate(&mut guard);
    drop(guard);

    eprintln!("已记录剪切板图片，{}×{}", saved.width, saved.height);
    let _ = app.emit("clip-added", ());
    if ocr_auto {
        crate::ocr::enqueue(id);
    }
}

fn recording_paused(app: &AppHandle) -> bool {
    app.try_state::<Mutex<AppState>>()
        .and_then(|state| state.lock().ok().map(|guard| guard.settings.pause_recording))
        .unwrap_or(false)
}

fn truncate(guard: &mut AppState) {
    if guard.clips.len() > 200 {
        guard.clips.truncate(200);
    }
}
