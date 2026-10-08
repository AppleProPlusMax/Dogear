use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, State};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    ShowWindow, SW_SHOW, SW_SHOWNORMAL,
};

use crate::clipboard;
use crate::state::AppState;

/// 图片条目放原图，其余放原文。`as_text` 为真时图片放识别出的文字。
enum Payload {
    Text(String),
    Image { png: Vec<u8>, hash: String },
}

fn payload(state: &State<'_, Mutex<AppState>>, id: i64, as_text: bool) -> Result<Payload, String> {
    let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
    let clip = guard
        .clips
        .iter()
        .find(|clip| clip.id == id)
        .ok_or("记录不存在")?;
    if clip.kind != "image" {
        return Ok(Payload::Text(clip.content.clone()));
    }
    if as_text {
        let text = clip
            .ocr_text
            .clone()
            .filter(|text| !text.trim().is_empty())
            .ok_or("这张图还没有识别出文字")?;
        return Ok(Payload::Text(text));
    }
    let path = clip.file_path.clone().ok_or("这条记录没有图片")?;
    let hash = clip.hash.clone().ok_or("这条记录没有图片")?;
    drop(guard);
    let png = std::fs::read(&path).map_err(|err| format!("无法读取图片: {err}"))?;
    Ok(Payload::Image { png, hash })
}

fn write_payload(payload: &Payload) -> Result<(), String> {
    match payload {
        Payload::Text(text) => clipboard::write_text(text),
        Payload::Image { png, hash } => clipboard::write_image(png, hash),
    }
}

#[tauri::command]
pub fn open_link(state: State<'_, Mutex<AppState>>, id: i64) -> Result<(), String> {
    let content = {
        let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
        let clip = guard
            .clips
            .iter()
            .find(|clip| clip.id == id)
            .ok_or("记录不存在")?;
        if clip.kind != "link" {
            return Err("这不是链接".into());
        }
        clip.content.clone()
    };
    let url = crate::detect::browser_url(&content).ok_or("这条链接打不开")?;
    open_in_browser(&url)
}

fn open_in_browser(url: &str) -> Result<(), String> {
    let operation = wide("open");
    let target = wide(url);
    let code = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if (code.0 as isize) <= 32 {
        return Err("无法在浏览器中打开".into());
    }
    Ok(())
}

fn wide(text: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[tauri::command]
pub fn copy_clip(state: State<'_, Mutex<AppState>>, id: i64, as_text: bool) -> Result<(), String> {
    write_payload(&payload(&state, id, as_text)?)
}

#[tauri::command]
pub fn paste_clip(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    id: i64,
    as_text: bool,
) -> Result<(), String> {
    let payload = payload(&state, id, as_text)?;
    let hwnd = {
        let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
        guard.previous_hwnd
    };

    write_payload(&payload)?;
    crate::panel::hide(&app);

    if hwnd == 0 {
        return Err("已复制，请手动粘贴".into());
    }
    send_to_window(hwnd)
}

fn send_to_window(hwnd: isize) -> Result<(), String> {
    let target = HWND(hwnd as *mut std::ffi::c_void);
    unsafe {
        focus_window(target);
        thread::sleep(Duration::from_millis(60));
        if GetForegroundWindow() != target {
            focus_window(target);
            thread::sleep(Duration::from_millis(40));
        }
        if GetForegroundWindow() != target {
            return Err("已复制，请手动粘贴".into());
        }
        send_ctrl_v().map_err(|err| format!("已复制，请手动粘贴（{err}）"))?;
    }
    Ok(())
}

pub(crate) unsafe fn focus_window(target: HWND) {
    let foreground = GetForegroundWindow();
    let current = GetCurrentThreadId();
    let foreground_thread = GetWindowThreadProcessId(foreground, None);
    let target_thread = GetWindowThreadProcessId(target, None);
    let attached_foreground = foreground_thread != 0
        && foreground_thread != current
        && AttachThreadInput(current, foreground_thread, true).0 != 0;
    let attached_target = target_thread != 0
        && target_thread != current
        && AttachThreadInput(current, target_thread, true).0 != 0;
    let _ = ShowWindow(target, SW_SHOW);
    let _ = SetForegroundWindow(target);
    let _ = BringWindowToTop(target);
    if attached_target {
        let _ = AttachThreadInput(current, target_thread, false);
    }
    if attached_foreground {
        let _ = AttachThreadInput(current, foreground_thread, false);
    }
}

unsafe fn send_ctrl_v() -> Result<(), String> {
    let inputs = [
        key_input(VK_CONTROL.0, Default::default()),
        key_input(b'V' as u16, Default::default()),
        key_input(b'V' as u16, KEYEVENTF_KEYUP),
        key_input(VK_CONTROL.0, KEYEVENTF_KEYUP),
    ];
    let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err(format!("SendInput 只发送了 {sent} 个事件"))
    }
}

fn key_input(vk: u16, flags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
