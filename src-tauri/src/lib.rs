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
mod storage;
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
            panel::set_image_viewer,
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
            panel::toggle_clip_pin,
            panel::delete_clip,
            panel::clear_clips,
            panel::storage_usage,
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
            let mut history = storage::init(app.handle(), loaded.history_limit as usize);
            let mut pending_ocr = Vec::new();
            for clip in &mut history.clips {
                if clip.kind == "image" && clip.ocr_status == "pending" {
                    if loaded.ocr_auto {
                        pending_ocr.push(clip.id);
                    } else {
                        clip.ocr_status = "none".into();
                        storage::update_ocr(clip.id, "none", None, None);
                    }
                }
            }
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
                clips: history.clips,
                next_id: history.next_id,
                effect,
                settings: loaded.clone(),
                ..AppState::default()
            }));
            settings::sync_launch(app.handle(), loaded.launch_at_login);
            panel::apply_auto_hide(app.handle(), loaded.auto_hide);

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
                    let (until, auto_hide, capturing) = state
                        .lock()
                        .map(|guard| {
                            (
                                guard.ignore_blur_until,
                                guard.settings.auto_hide,
                                guard.capture_active,
                            )
                        })
                        .unwrap_or((0, false, false));
                    // 截图层会抢走焦点。自动隐藏开着时，这次失焦不能当成用户离开。
                    if capturing {
                        return;
                    }
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
            for id in pending_ocr {
                ocr::enqueue(id);
            }
            watcher::start(app.handle().clone());
            if should_present_on_launch(&std::env::args().collect::<Vec<_>>(), loaded.guide_seen) {
                panel::open(app.handle());
            }
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
        panel::nudge_surface(&window);
    }
}

/// 快捷方式、开始菜单打开时显示窗口。开机自启带 `--autostart`，只进托盘。
/// 第一次使用即使是开机自启也要打开，否则教程出不来。
fn should_present_on_launch(args: &[String], guide_seen: bool) -> bool {
    let from_login = args.iter().any(|arg| arg == "--autostart");
    !from_login || !guide_seen
}

#[cfg(test)]
mod tests {
    use super::should_present_on_launch;

    #[test]
    fn shortcut_opens_the_window_and_login_stays_in_the_tray() {
        let shortcut = vec!["dogear.exe".into()];
        let login = vec!["dogear.exe".into(), "--autostart".into()];
        assert!(should_present_on_launch(&shortcut, true));
        assert!(should_present_on_launch(&shortcut, false));
        assert!(!should_present_on_launch(&login, true));
        assert!(should_present_on_launch(&login, false));
    }
}
