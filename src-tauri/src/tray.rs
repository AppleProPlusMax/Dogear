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
static OPEN: OnceLock<MenuItem<Wry>> = OnceLock::new();
static CAPTURE: OnceLock<MenuItem<Wry>> = OnceLock::new();
static SETTINGS_ITEM: OnceLock<MenuItem<Wry>> = OnceLock::new();
static QUIT: OnceLock<MenuItem<Wry>> = OnceLock::new();

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let (paused, language) = app
        .try_state::<Mutex<AppState>>()
        .and_then(|state| {
            state.lock().ok().map(|guard| {
                (guard.settings.pause_recording, guard.settings.language.clone())
            })
        })
        .unwrap_or((false, "zh-CN".into()));
    let labels = tray_labels(&language);
    let open = MenuItem::with_id(app, "open", labels.open, true, None::<&str>)?;
    let capture = MenuItem::with_id(app, "capture", labels.capture, true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", labels.pause, true, paused, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", labels.settings, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", labels.quit, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &capture, &pause, &settings_item, &quit])?;
    let _ = PAUSE.set(pause);
    let _ = OPEN.set(open);
    let _ = CAPTURE.set(capture);
    let _ = SETTINGS_ITEM.set(settings_item);
    let _ = QUIT.set(quit);
    let tray = TrayIconBuilder::with_id("dogear")
        .icon(dogear_icon()?)
        .tooltip("Dogear")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => panel::open(app),
            "capture" => crate::capture::begin(app),
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

pub fn apply_language(language: &str) {
    let labels = tray_labels(language);
    if let Some(item) = OPEN.get() {
        let _ = item.set_text(labels.open);
    }
    if let Some(item) = CAPTURE.get() {
        let _ = item.set_text(labels.capture);
    }
    if let Some(item) = PAUSE.get() {
        let _ = item.set_text(labels.pause);
    }
    if let Some(item) = SETTINGS_ITEM.get() {
        let _ = item.set_text(labels.settings);
    }
    if let Some(item) = QUIT.get() {
        let _ = item.set_text(labels.quit);
    }
}

struct TrayLabels {
    open: &'static str,
    capture: &'static str,
    pause: &'static str,
    settings: &'static str,
    quit: &'static str,
}

fn tray_labels(language: &str) -> TrayLabels {
    if language == "en" {
        TrayLabels {
            open: "Open",
            capture: "Screenshot",
            pause: "Pause recording",
            settings: "Settings",
            quit: "Quit",
        }
    } else {
        TrayLabels {
            open: "打开",
            capture: "截图",
            pause: "暂停记录",
            settings: "设置",
            quit: "退出",
        }
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
