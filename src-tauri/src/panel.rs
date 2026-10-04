use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::SetWindowRgn;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetForegroundWindow, GetWindowRect};

use crate::paste;
use crate::state::{self, AppState};

static ESCAPE_WANTED: AtomicBool = AtomicBool::new(false);

fn escape_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 面板可见时才占用 Esc。注册必须离开主线程，否则会和快捷键回调互相卡住。
fn set_escape(app: &AppHandle, enabled: bool) {
    ESCAPE_WANTED.store(enabled, Ordering::SeqCst);
    let app = app.clone();
    thread::spawn(move || {
        let _guard = escape_lock().lock().unwrap_or_else(|err| err.into_inner());
        let shortcuts = app.global_shortcut();
        let want = ESCAPE_WANTED.load(Ordering::SeqCst);
        let registered = shortcuts.is_registered("Escape");
        if want == registered {
            return;
        }
        let result = if want {
            shortcuts.register("Escape")
        } else {
            shortcuts.unregister("Escape")
        };
        if let Err(err) = result {
            eprintln!("Esc 快捷键同步失败: {err}");
        }
    });
}

pub fn capture_foreground(state: &Mutex<AppState>) {
    let hwnd = unsafe { GetForegroundWindow() };
    if let Ok(mut guard) = state.lock() {
        guard.previous_hwnd = hwnd.0 as isize;
    }
}

pub fn toggle(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        hide(app);
        return;
    }
    show(app, &window);
}

pub fn open(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    show(app, &window);
}

pub fn hide(app: &AppHandle) {
    set_escape(app, false);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn show(app: &AppHandle, window: &tauri::WebviewWindow) {
    if let Some(state) = app.try_state::<Mutex<AppState>>() {
        capture_foreground(&state);
        if let Ok(mut guard) = state.lock() {
            guard.ignore_blur_until = state::now_ms() + 400;
        }
    }
    position_near_cursor(window);
    let _ = window.show();
    if let Ok(hwnd) = window.hwnd() {
        unsafe { paste::focus_window(HWND(hwnd.0)) };
    }
    let _ = window.set_focus();
    apply_system_corners(window);
    set_escape(app, true);
    let _ = app.emit("window-shown", ());
}

/// 交给 Windows 11 画圆角。半径与 `tokens.css` 的 `--r-panel: 8px` 对齐。
pub fn apply_system_corners(window: &tauri::WebviewWindow) {
    let Ok(raw) = window.hwnd() else {
        return;
    };
    let hwnd = HWND(raw.0);
    unsafe {
        let _ = SetWindowRgn(hwnd, None, true);
        let preference = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const _,
            std::mem::size_of_val(&preference) as u32,
        );
        let border = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border as *const _ as *const _,
            std::mem::size_of_val(&border) as u32,
        );
    }
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) -> Result<(), String> {
    hide(&app);
    Ok(())
}

/// 拖动开始前由前端调用。Windows 用标题栏拖动时会先 ReleaseCapture，窗口会误报失焦。
#[tauri::command]
pub fn arm_window_drag(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
    guard.drag_blur_until = state::now_ms() + 800;
    Ok(())
}

pub fn drag_blur_active(state: &Mutex<AppState>) -> bool {
    let until = state
        .lock()
        .map(|guard| guard.drag_blur_until)
        .unwrap_or(0);
    state::now_ms() < until
}

/// 鼠标还按在窗口里：失焦是拖动，不是点到了别的程序。
pub fn pointer_is_dragging(window: &tauri::WebviewWindow) -> bool {
    if !left_button_down() {
        return false;
    }
    let Ok(raw) = window.hwnd() else {
        return false;
    };
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(HWND(raw.0), &mut rect) }.is_err() {
        return false;
    }
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return false;
    }
    point.x >= rect.left
        && point.x < rect.right
        && point.y >= rect.top
        && point.y < rect.bottom
}

/// 拖动结束后把焦点还给面板，否则之后点外面不会再触发隐藏。
pub fn refocus_after_drag(app: AppHandle) {
    thread::spawn(move || {
        let start = state::now_ms();
        while left_button_down() && state::now_ms() - start < 30_000 {
            thread::sleep(std::time::Duration::from_millis(16));
        }
        thread::sleep(std::time::Duration::from_millis(32));
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        if !window.is_visible().unwrap_or(false) {
            return;
        }
        if let Ok(hwnd) = window.hwnd() {
            unsafe { paste::focus_window(HWND(hwnd.0)) };
        }
        let _ = window.set_focus();
    });
}

fn left_button_down() -> bool {
    unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON.0)) < 0 }
}

#[tauri::command]
pub fn list_clips(state: State<'_, Mutex<AppState>>) -> Result<Vec<crate::state::Clip>, String> {
    let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
    Ok(guard.clips.clone())
}

/// 图片原图的 data URL。路径不交给前端，避免 webview 直接访问文件系统。
#[tauri::command]
pub fn clip_image(state: State<'_, Mutex<AppState>>, id: i64) -> Result<String, String> {
    let path = clip_file(&state, id, false)?;
    crate::images::read_as_data_url(&path)
}

#[tauri::command]
pub fn clip_thumb(state: State<'_, Mutex<AppState>>, id: i64) -> Result<String, String> {
    let path = clip_file(&state, id, true)?;
    crate::images::read_as_data_url(&path)
}

/// 手动重新识别（Ctrl+O 或预览区按钮）。
#[tauri::command]
pub fn retry_ocr(state: State<'_, Mutex<AppState>>, id: i64) -> Result<(), String> {
    {
        let mut guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
        let clip = guard
            .clips
            .iter_mut()
            .find(|clip| clip.id == id)
            .ok_or("记录不存在")?;
        if clip.kind != "image" {
            return Err("只有图片可以识别文字".into());
        }
        clip.ocr_status = "pending".into();
        clip.ocr_text = None;
    }
    crate::ocr::enqueue(id);
    Ok(())
}

fn clip_file(
    state: &State<'_, Mutex<AppState>>,
    id: i64,
    thumb: bool,
) -> Result<std::path::PathBuf, String> {
    let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
    let clip = guard
        .clips
        .iter()
        .find(|clip| clip.id == id)
        .ok_or("记录不存在")?;
    let path = if thumb {
        clip.thumb_path.as_ref()
    } else {
        clip.file_path.as_ref()
    };
    path.map(std::path::PathBuf::from).ok_or_else(|| "这条记录没有图片".into())
}

fn position_near_cursor(window: &tauri::WebviewWindow) {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return;
    }
    let mut x = point.x;
    let mut y = point.y + 12;
    if let Ok(Some(monitor)) = window.monitor_from_point(x as f64, y as f64) {
        let area = monitor.work_area();
        let win = window.outer_size().unwrap_or(tauri::PhysicalSize::new(780, 640));
        let max_x = area.position.x + area.size.width as i32 - win.width as i32;
        let max_y = area.position.y + area.size.height as i32 - win.height as i32;
        x = x.clamp(area.position.x, max_x.max(area.position.x));
        y = y.clamp(area.position.y, max_y.max(area.position.y));
    }
    let _ = window.set_position(PhysicalPosition::new(x, y));
}
