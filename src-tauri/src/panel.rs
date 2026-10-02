use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetForegroundWindow};

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
    set_escape(app, true);
    let _ = app.emit("window-shown", ());
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) -> Result<(), String> {
    hide(&app);
    Ok(())
}

#[tauri::command]
pub fn list_clips(state: State<'_, Mutex<AppState>>) -> Result<Vec<crate::state::Clip>, String> {
    let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
    Ok(guard.clips.clone())
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
