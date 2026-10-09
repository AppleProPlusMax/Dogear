mod capture;
mod clipboard;
mod detect;
mod effects;
mod images;
mod ocr;
mod panel;
mod paste;
mod settings;
mod state;
mod tray;
mod watcher;

use std::sync::Mutex;

use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::settings::Settings;
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

#[tauri::command]
fn get_settings(state: tauri::State<'_, Mutex<AppState>>) -> Result<Settings, String> {
    let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
    Ok(guard.settings.clone())
}

#[tauri::command]
fn update_settings(app: tauri::AppHandle, patch: settings::SettingsPatch) -> Result<Settings, String> {
    settings::update(&app, patch)
}

#[tauri::command]
fn begin_shortcut_capture(app: tauri::AppHandle) -> Result<(), String> {
    settings::begin_shortcut_capture(&app)
}

#[tauri::command]
fn end_shortcut_capture(app: tauri::AppHandle) -> Result<(), String> {
    settings::end_shortcut_capture(&app)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            panel::open(app);
        }))
        .invoke_handler(tauri::generate_handler![
            panel::list_clips,
            panel::hide_panel,
            panel::minimize_panel,
            panel::arm_window_drag,
            panel::clip_image,
            panel::clip_thumb,
            panel::retry_ocr,
            paste::copy_clip,
            paste::open_link,
            paste::paste_clip,
            paste::preview_code,
            paste::clip_colors,
            paste::copy_plain,
            capture::start_capture,
            get_effect_state,
            get_settings,
            update_settings,
            begin_shortcut_capture,
            end_shortcut_capture,
            ocr_image
        ])
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .ok_or("缺少主窗口 main")?;
            let loaded = settings::load();
            let effect = effects::apply(&window, loaded.reduce_transparency);
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
                settings: loaded.clone(),
                ..AppState::default()
            }));
            settings::sync_launch(app.handle(), loaded.launch_at_login);
            panel::apply_auto_hide(app.handle(), loaded.auto_hide);
            if !loaded.guide_seen {
                panel::open(app.handle());
            }

            let blur_handle = app.handle().clone();
            window.on_window_event(move |event| match event {
                WindowEvent::Focused(true) => {
                    let Some(window) = blur_handle.get_webview_window("main") else {
                        return;
                    };
                    if window.is_minimized().unwrap_or(false) {
                        return;
                    }
                    panel::hold_escape(&blur_handle);
                }
                WindowEvent::Focused(false) => {
                    let Some(state) = blur_handle.try_state::<Mutex<AppState>>() else {
                        return;
                    };
                    let (until, auto_hide) = state
                        .lock()
                        .map(|guard| (guard.ignore_blur_until, guard.settings.auto_hide))
                        .unwrap_or((0, false));
                    if !auto_hide {
                        return;
                    }
                    if state::now_ms() < until {
                        return;
                    }
                    let dragging = panel::drag_blur_active(&state)
                        || blur_handle
                            .get_webview_window("main")
                            .is_some_and(|window| panel::pointer_is_dragging(&window));
                    if dragging {
                        panel::refocus_after_drag(blur_handle.clone());
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

            if let Err(err) = tray::install(app.handle()) {
                eprintln!("托盘图标创建失败: {err}");
            }
            register_shortcut(app.handle())?;
            ocr::start_queue(app.handle().clone());
            watcher::start(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn register_shortcut(app: &tauri::AppHandle) -> tauri::Result<()> {
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            settings::on_global_shortcut(app, shortcut);
        })
        .build();

    if let Err(err) = app.plugin(plugin) {
        eprintln!("全局快捷键加载失败: {err}。窗口将直接显示，便于继续验证界面。");
        reveal_window(app);
        return Ok(());
    }
    let configured = app
        .state::<Mutex<AppState>>()
        .lock()
        .map(|guard| guard.settings.clone())
        .unwrap_or_default();
    if let Err(err) = settings::register_user_shortcuts(app, &configured) {
        eprintln!("快捷键注册失败: {err}。窗口将直接显示，便于继续验证界面。");
        reveal_window(app);
    }
    Ok(())
}

fn reveal_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
