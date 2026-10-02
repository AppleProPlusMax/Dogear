use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetForegroundWindow};

use crate::state::{self, AppState};

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
    let visible = window.is_visible().unwrap_or(false);
    if visible {
        let _ = window.hide();
        return;
    }
    if let Some(state) = app.try_state::<Mutex<AppState>>() {
        capture_foreground(&state);
        if let Ok(mut guard) = state.lock() {
            guard.ignore_blur_until = state::now_ms() + 400;
        }
    }
    position_near_cursor(&window);
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit("window-shown", ());
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|err| err.to_string())?;
    }
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
        let win = window.outer_size().unwrap_or(tauri::PhysicalSize::new(420, 520));
        let max_x = area.position.x + area.size.width as i32 - win.width as i32;
        let max_y = area.position.y + area.size.height as i32 - win.height as i32;
        x = x.clamp(area.position.x, max_x.max(area.position.x));
        y = y.clamp(area.position.y, max_y.max(area.position.y));
    }
    let _ = window.set_position(PhysicalPosition::new(x, y));
}
