use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, State};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow,
    SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW, SW_SHOWNORMAL,
};

use crate::clipboard;
use crate::state::AppState;

/// 同一次只允许一个复制或粘贴去动剪切板和键盘，避免两下叠在一起卡住。
static CLIPBOARD_ACTION: Mutex<()> = Mutex::new(());

fn try_clipboard_action() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    CLIPBOARD_ACTION
        .try_lock()
        .map_err(|_| "上一次复制或粘贴还没结束".to_string())
}

/// 图片条目放原图，其余放原文。`as_text` 为真时图片放识别出的文字。
enum Payload {
    Text(String),
    Image { png: Vec<u8>, hash: String },
}

fn payload(state: &State<'_, Mutex<AppState>>, id: i64, as_text: bool, view: &str) -> Result<Payload, String> {
    let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
    let clip = guard
        .clips
        .iter()
        .find(|clip| clip.id == id)
        .ok_or("记录不存在")?;
    if clip.kind != "image" {
        let text = if clip.kind == "code" {
            crate::detect::present_code(&clip.content, clip.language.as_deref(), view)
        } else {
            clip.content.clone()
        };
        return Ok(Payload::Text(text));
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
pub fn clip_colors(state: State<'_, Mutex<AppState>>, id: i64) -> Result<Vec<crate::detect::FoundColor>, String> {
    let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
    let clip = guard
        .clips
        .iter()
        .find(|clip| clip.id == id)
        .ok_or("记录不存在")?;
    if clip.kind != "text" && clip.kind != "code" {
        return Ok(Vec::new());
    }
    Ok(crate::detect::find_colors(&clip.content))
}

#[tauri::command]
pub fn copy_plain(text: String) -> Result<(), String> {
    if text.chars().count() > 80 {
        return Err("只能复制较短的颜色值".into());
    }
    clipboard::write_text(&text)
}

#[tauri::command]
pub fn preview_code(state: State<'_, Mutex<AppState>>, id: i64, view: String) -> Result<String, String> {
    let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
    let clip = guard
        .clips
        .iter()
        .find(|clip| clip.id == id)
        .ok_or("记录不存在")?;
    Ok(crate::detect::present_code(
        &clip.content,
        clip.language.as_deref(),
        &view,
    ))
}

#[tauri::command]
pub async fn copy_clip(state: State<'_, Mutex<AppState>>, id: i64, as_text: bool, view: Option<String>) -> Result<(), String> {
    let data = payload(&state, id, as_text, view.as_deref().unwrap_or("raw"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let _gate = try_clipboard_action()?;
        write_payload(&data)
    })
    .await
    .map_err(|_| "复制被中断".to_string())?
}

#[tauri::command]
pub async fn paste_clip(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    id: i64,
    as_text: bool,
    view: Option<String>,
) -> Result<(), String> {
    let data = payload(&state, id, as_text, view.as_deref().unwrap_or("raw"))?;
    let hwnd = {
        let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
        guard.previous_hwnd
    };
    tauri::async_runtime::spawn_blocking(move || paste_into(app, data, hwnd))
        .await
        .map_err(|_| "粘贴被中断".to_string())?
}

fn paste_into(app: AppHandle, data: Payload, hwnd: isize) -> Result<(), String> {
    let _gate = try_clipboard_action()?;
    write_payload(&data)?;
    if !can_auto_paste(hwnd, target_pid(hwnd), std::process::id(), window_exists(hwnd)) {
        return Err("已复制，请手动粘贴".into());
    }
    crate::panel::hide(&app);
    thread::sleep(Duration::from_millis(40));
    let target = HWND(hwnd as *mut std::ffi::c_void);
    if !unsafe { focus_for_paste(target) } {
        crate::panel::reveal_after_failed_paste(&app);
        return Err("已复制，请手动粘贴".into());
    }
    unsafe { send_ctrl_v() }.map_err(|err| format!("已复制，请手动粘贴（{err}）"))
}

fn can_auto_paste(hwnd: isize, target_pid: u32, our_pid: u32, window_exists: bool) -> bool {
    hwnd != 0 && window_exists && target_pid != 0 && target_pid != our_pid
}

fn window_exists(hwnd: isize) -> bool {
    hwnd != 0 && unsafe { IsWindow(Some(HWND(hwnd as *mut std::ffi::c_void))).as_bool() }
}

fn target_pid(hwnd: isize) -> u32 {
    if hwnd == 0 {
        return 0;
    }
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(hwnd as *mut std::ffi::c_void), Some(&mut pid));
        pid
    }
}

/// 切到目标窗口。不用 AttachThreadInput：那会和界面线程的消息循环卡在一起。
unsafe fn focus_for_paste(target: HWND) -> bool {
    let show = show_command(IsIconic(target).as_bool());
    let _ = ShowWindow(target, show);
    // 只松开 Alt，不按下。按下一整下会打开目标程序的菜单。
    for tap in foreground_unlock_taps() {
        let flags = if tap.key_up { KEYEVENTF_KEYUP } else { Default::default() };
        keybd_event(tap.vk, 0, flags, 0);
    }
    let _ = SetForegroundWindow(target);
    let _ = BringWindowToTop(target);
    thread::sleep(Duration::from_millis(40));
    if GetForegroundWindow() == target {
        return true;
    }
    let _ = SetForegroundWindow(target);
    thread::sleep(Duration::from_millis(40));
    GetForegroundWindow() == target
}

fn show_command(is_minimized: bool) -> windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD {
    if is_minimized { SW_RESTORE } else { SW_SHOW }
}

struct KeyTap {
    vk: u8,
    key_up: bool,
}

fn foreground_unlock_taps() -> &'static [KeyTap] {
    &[KeyTap { vk: 0x12, key_up: true }]
}

pub(crate) unsafe fn focus_window(target: HWND) {
    // 和粘贴切焦点同一条路：只显示、只松开 Alt，不把输入线程绑在一起。
    let show = show_command(IsIconic(target).as_bool());
    let _ = ShowWindow(target, show);
    for tap in foreground_unlock_taps() {
        let flags = if tap.key_up { KEYEVENTF_KEYUP } else { Default::default() };
        keybd_event(tap.vk, 0, flags, 0);
    }
    let _ = SetForegroundWindow(target);
    let _ = BringWindowToTop(target);
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

#[cfg(test)]
mod tests {
    use super::{can_auto_paste, foreground_unlock_taps, show_command};
    use super::{SW_RESTORE, SW_SHOW};

    #[test]
    fn paste_skips_missing_and_own_windows() {
        assert!(!can_auto_paste(0, 10, 20, true));
        assert!(!can_auto_paste(5, 20, 20, true));
        assert!(!can_auto_paste(5, 10, 20, false));
        assert!(can_auto_paste(5, 10, 20, true));
    }

    #[test]
    fn maximized_windows_stay_maximized_and_alt_is_only_released() {
        assert_eq!(show_command(false), SW_SHOW);
        assert_eq!(show_command(true), SW_RESTORE);
        let taps = foreground_unlock_taps();
        assert!(taps.iter().all(|tap| tap.key_up));
        assert!(taps.iter().any(|tap| tap.vk == 0x12));
    }
}
