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
use crate::state::{self, AppState};

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
                let clip = crate::state::Clip {
                    id: guard.next_id,
                    content: text,
                    created_at: now,
                };
                guard.next_id += 1;
                guard.clips.insert(0, clip);
                if guard.clips.len() > 200 {
                    guard.clips.truncate(200);
                }
            }
            let chars = guard.clips.first().map(|clip| clip.content.chars().count()).unwrap_or(0);
            drop(guard);
            eprintln!("已记录剪切板文本，长度 {chars} 字");
            let _ = app.emit("clip-added", ());
        }
        ClipboardRead::Ignored => {
            eprintln!("已跳过一条剪切板内容（敏感标记或超出长度上限），未记录正文");
        }
        ClipboardRead::Empty => {}
    }
}
