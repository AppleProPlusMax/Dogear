use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::panel;

const DOGEAR_ICON: &[u8] = include_bytes!("../icons/128x128.png");

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let tray = TrayIconBuilder::with_id("dogear")
        .icon(dogear_icon()?)
        .tooltip("Dogear")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => panel::open(app),
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

fn dogear_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(DOGEAR_ICON)
}
