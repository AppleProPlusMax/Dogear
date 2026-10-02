mod clipboard;
mod effects;
mod ocr;
mod panel;
mod paste;
mod state;
mod watcher;

use std::sync::Mutex;

use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};

use crate::state::AppState;

#[tauri::command]
fn get_effect_state(state: tauri::State<'_, Mutex<AppState>>) -> Result<effects::EffectState, String> {
    let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
    Ok(guard.effect.clone())
}

#[tauri::command]
fn ocr_image(path: String) -> Result<ocr::OcrReport, String> {
    ocr::recognize_png(std::path::Path::new(&path))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            panel::list_clips,
            panel::hide_panel,
            paste::copy_clip,
            paste::paste_clip,
            get_effect_state,
            ocr_image
        ])
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .ok_or("缺少主窗口 main")?;
            let effect = effects::apply(&window);
            panel::apply_system_corners(&window);
            eprintln!(
                "窗口效果: {}（build {}，透明 {}，高对比度 {}）{}",
                effect.effect,
                effect.windows_build,
                effect.transparency_enabled,
                effect.high_contrast,
                effect.detail
            );
            app.manage(Mutex::new(AppState {
                effect,
                ..AppState::default()
            }));

            let blur_handle = app.handle().clone();
            window.on_window_event(move |event| match event {
                WindowEvent::Focused(false) => {
                    let Some(state) = blur_handle.try_state::<Mutex<AppState>>() else {
                        return;
                    };
                    let until = state.lock().map(|guard| guard.ignore_blur_until).unwrap_or(0);
                    if state::now_ms() < until {
                        return;
                    }
                    panel::hide(&blur_handle);
                }
                WindowEvent::Resized(size) if size.width >= 8 && size.height >= 8 => {
                    if let Some(window) = blur_handle.get_webview_window("main") {
                        panel::apply_system_corners(&window);
                    }
                }
                _ => {}
            });

            register_shortcut(app.handle())?;
            watcher::start(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn register_shortcut(app: &tauri::AppHandle) -> tauri::Result<()> {
    let builder = match tauri_plugin_global_shortcut::Builder::new().with_shortcuts(["Alt+V"]) {
        Ok(builder) => builder,
        Err(err) => {
            eprintln!("Alt+V 注册失败: {err}。窗口将直接显示，便于继续验证界面。");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            return Ok(());
        }
    };
    let plugin = builder
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            if shortcut.matches(Modifiers::ALT, Code::KeyV) {
                panel::toggle(app);
            } else if shortcut.matches(Modifiers::empty(), Code::Escape) {
                panel::hide(app);
            }
        })
        .build();

    if let Err(err) = app.plugin(plugin) {
        eprintln!("Alt+V 注册失败: {err}。窗口将直接显示，便于继续验证界面。");
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
    Ok(())
}
