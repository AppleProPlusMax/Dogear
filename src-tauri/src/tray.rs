use std::sync::{Mutex, OnceLock};
use std::thread;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::panel;
use crate::settings::{self, SettingsPatch};
use crate::state::AppState;

const DOGEAR_ICON: &[u8] = include_bytes!("../icons/128x128.png");

static PAUSE: OnceLock<CheckMenuItem<Wry>> = OnceLock::new();

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let paused = app
        .try_state::<Mutex<AppState>>()
        .and_then(|state| state.lock().ok().map(|guard| guard.settings.pause_recording))
        .unwrap_or(false);
    let open = MenuItem::with_id(app, "open", "打开", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "暂停记录", true, paused, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pause, &settings_item, &quit])?;
    let _ = PAUSE.set(pause);
    let tray = TrayIconBuilder::with_id("dogear")
        .icon(dogear_icon()?)
        .tooltip("Dogear")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => panel::open(app),
            "pause" => toggle_pause(app),
            "settings" => panel::open_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                panel::open(tray.app_handle());
            }
        })
        .build(app)?;
    app.manage(tray);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_icon(dogear_icon()?);
    }
    Ok(())
}

pub fn set_paused(paused: bool) {
    if let Some(item) = PAUSE.get() {
        let _ = item.set_checked(paused);
    }
}

fn toggle_pause(app: &AppHandle) {
    let Some(item) = PAUSE.get() else {
        return;
    };
    let checked = item.is_checked().unwrap_or(false);
    let app = app.clone();
    thread::spawn(move || {
        let patch = SettingsPatch {
            pause_recording: Some(checked),
            ..SettingsPatch::default()
        };
        if let Err(err) = settings::update(&app, patch) {
            eprintln!("暂停记录失败: {err}");
            if let Some(item) = PAUSE.get() {
                let _ = item.set_checked(!checked);
            }
        }
    });
}

fn dogear_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(DOGEAR_ICON)
}
