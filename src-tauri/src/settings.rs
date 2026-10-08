use std::fs;
use std::path::PathBuf;
use std::thread;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use crate::capture;
use crate::panel;
use crate::state::AppState;
use crate::tray;

const SUMMON_DEFAULT: &str = "Alt+V";
const CAPTURE_DEFAULT: &str = "Alt+C";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub summon_shortcut: String,
    pub capture_shortcut: String,
    pub launch_at_login: bool,
    pub pause_recording: bool,
    pub theme: String,
    pub reduce_transparency: bool,
    pub ocr_auto: bool,
    /// 失焦后收起窗口。默认关着，窗口留在任务栏，用缩小和关闭来收起。
    pub auto_hide: bool,
    /// 已经看过或跳过首次教程。
    pub guide_seen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            summon_shortcut: SUMMON_DEFAULT.into(),
            capture_shortcut: CAPTURE_DEFAULT.into(),
            launch_at_login: false,
            pause_recording: false,
            theme: "system".into(),
            reduce_transparency: false,
            ocr_auto: true,
            auto_hide: false,
            guide_seen: false,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    pub summon_shortcut: Option<String>,
    pub capture_shortcut: Option<String>,
    pub launch_at_login: Option<bool>,
    pub pause_recording: Option<bool>,
    pub theme: Option<String>,
    pub reduce_transparency: Option<bool>,
    pub ocr_auto: Option<bool>,
    pub auto_hide: Option<bool>,
    pub guide_seen: Option<bool>,
}

pub fn load() -> Settings {
    let path = match settings_path() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("设置目录不可用，使用默认值: {err}");
            return Settings::default();
        }
    };
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(err) => {
            eprintln!("设置文件无法读取，使用默认值: {err}");
            return Settings::default();
        }
    };
    match serde_json::from_str::<Settings>(&text) {
        Ok(settings) => normalize_loaded(settings),
        Err(err) => {
            eprintln!("设置文件无法解析，使用默认值: {err}");
            Settings::default()
        }
    }
}

pub fn update(app: &AppHandle, patch: SettingsPatch) -> Result<Settings, String> {
    let current = current(app)?;
    let next = merge(current.clone(), patch)?;

    if next.summon_shortcut != current.summon_shortcut || next.capture_shortcut != current.capture_shortcut {
        sync_shortcuts(app, &current, &next).map_err(|err| {
            let _ = register_user_shortcuts(app, &current);
            err
        })?;
    }
    if next.launch_at_login != current.launch_at_login {
        if let Err(err) = apply_launch(app, next.launch_at_login) {
            let _ = sync_shortcuts(app, &next, &current);
            return Err(err);
        }
    }
    if next.reduce_transparency != current.reduce_transparency {
        if let Err(err) = reapply_effect(app, next.reduce_transparency) {
            let _ = sync_shortcuts(app, &next, &current);
            let _ = apply_launch(app, current.launch_at_login);
            return Err(err);
        }
    }
    if let Err(err) = save(&next) {
        let _ = sync_shortcuts(app, &next, &current);
        let _ = apply_launch(app, current.launch_at_login);
        if next.reduce_transparency != current.reduce_transparency {
            let _ = reapply_effect(app, current.reduce_transparency);
        }
        return Err(err);
    }

    {
        let state = app.state::<std::sync::Mutex<AppState>>();
        let mut guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
        guard.settings = next.clone();
        guard.shortcut_capture = false;
    }
    tray::set_paused(next.pause_recording);
    if next.auto_hide != current.auto_hide {
        panel::apply_auto_hide(app, next.auto_hide);
    }
    let _ = app.emit("settings-changed", &next);
    Ok(next)
}

pub fn sync_launch(app: &AppHandle, enabled: bool) {
    if let Err(err) = apply_launch(app, enabled) {
        eprintln!("开机自启同步失败: {err}");
    }
}

pub fn register_user_shortcuts(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    for shortcut in [&settings.summon_shortcut, &settings.capture_shortcut] {
        if shortcuts.is_registered(shortcut.as_str()) {
            continue;
        }
        shortcuts
            .register(shortcut.as_str())
            .map_err(|err| format!("{shortcut} 注册失败: {err}"))?;
    }
    Ok(())
}

pub fn begin_shortcut_capture(app: &AppHandle) -> Result<(), String> {
    let settings = {
        let state = app.state::<std::sync::Mutex<AppState>>();
        let mut guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
        guard.shortcut_capture = true;
        guard.settings.clone()
    };
    unregister_user_shortcuts(app, &settings)
}

pub fn end_shortcut_capture(app: &AppHandle) -> Result<(), String> {
    let settings = {
        let state = app.state::<std::sync::Mutex<AppState>>();
        let mut guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
        guard.shortcut_capture = false;
        guard.settings.clone()
    };
    register_user_shortcuts(app, &settings)
}

/// 快捷键回调里不能同步注册，否则会卡在主线程上。
pub fn abort_shortcut_capture(app: &AppHandle) {
    let settings = {
        let Some(state) = app.try_state::<std::sync::Mutex<AppState>>() else {
            return;
        };
        let Ok(mut guard) = state.lock() else {
            return;
        };
        if !guard.shortcut_capture {
            return;
        }
        guard.shortcut_capture = false;
        guard.settings.clone()
    };
    let app = app.clone();
    thread::spawn(move || {
        if let Err(err) = register_user_shortcuts(&app, &settings) {
            eprintln!("恢复快捷键失败: {err}");
        }
        let _ = app.emit("shortcut-capture-cancel", ());
    });
}

pub fn on_global_shortcut(app: &AppHandle, shortcut: &Shortcut) {
    if shortcut.matches(Modifiers::empty(), Code::Escape) {
        let capturing = app
            .try_state::<std::sync::Mutex<AppState>>()
            .and_then(|state| state.lock().ok().map(|guard| guard.shortcut_capture))
            .unwrap_or(false);
        if capturing {
            abort_shortcut_capture(app);
            return;
        }
        panel::hide(app);
        return;
    }

    let Some((summon, capture_shortcut)) = configured_shortcuts(app) else {
        return;
    };
    if binds(shortcut, &summon) {
        panel::toggle(app);
    } else if binds(shortcut, &capture_shortcut) {
        capture::begin(app);
    }
}

fn current(app: &AppHandle) -> Result<Settings, String> {
    let state = app.state::<std::sync::Mutex<AppState>>();
    let guard = state.lock().map_err(|_| "状态锁失败".to_string())?;
    Ok(guard.settings.clone())
}

fn configured_shortcuts(app: &AppHandle) -> Option<(String, String)> {
    let state = app.try_state::<std::sync::Mutex<AppState>>()?;
    let guard = state.lock().ok()?;
    Some((
        guard.settings.summon_shortcut.clone(),
        guard.settings.capture_shortcut.clone(),
    ))
}

fn merge(current: Settings, patch: SettingsPatch) -> Result<Settings, String> {
    let mut next = current;
    if let Some(shortcut) = patch.summon_shortcut {
        next.summon_shortcut = normalize_shortcut(&shortcut)?;
    }
    if let Some(shortcut) = patch.capture_shortcut {
        next.capture_shortcut = normalize_shortcut(&shortcut)?;
    }
    if let Some(value) = patch.launch_at_login {
        next.launch_at_login = value;
    }
    if let Some(value) = patch.pause_recording {
        next.pause_recording = value;
    }
    if let Some(theme) = patch.theme {
        next.theme = normalize_theme(&theme)?;
    }
    if let Some(value) = patch.reduce_transparency {
        next.reduce_transparency = value;
    }
    if let Some(value) = patch.ocr_auto {
        next.ocr_auto = value;
    }
    if let Some(value) = patch.auto_hide {
        next.auto_hide = value;
    }
    if let Some(value) = patch.guide_seen {
        next.guide_seen = value;
    }
    if next.summon_shortcut == next.capture_shortcut {
        return Err("呼出和截图不能使用同一个快捷键".into());
    }
    Ok(next)
}

fn normalize_loaded(mut settings: Settings) -> Settings {
    settings.summon_shortcut = normalize_shortcut(&settings.summon_shortcut).unwrap_or_else(|_| SUMMON_DEFAULT.into());
    settings.capture_shortcut = normalize_shortcut(&settings.capture_shortcut).unwrap_or_else(|_| CAPTURE_DEFAULT.into());
    if settings.summon_shortcut == settings.capture_shortcut {
        settings.summon_shortcut = SUMMON_DEFAULT.into();
        settings.capture_shortcut = CAPTURE_DEFAULT.into();
    }
    if normalize_theme(&settings.theme).is_err() {
        settings.theme = "system".into();
    }
    settings
}

fn normalize_theme(theme: &str) -> Result<String, String> {
    match theme {
        "system" | "light" | "dark" => Ok(theme.to_string()),
        _ => Err("主题只能是跟随系统、浅色或深色".into()),
    }
}

fn normalize_shortcut(raw: &str) -> Result<String, String> {
    let prepared = prepare_shortcut(raw);
    let shortcut: Shortcut = prepared
        .parse()
        .map_err(|_| format!("无法识别快捷键 {raw}"))?;
    if shortcut.mods.is_empty() {
        return Err("请至少带上 Ctrl、Alt、Shift 或 Win".into());
    }
    if shortcut.key == Code::Escape {
        return Err("Esc 用来关闭窗口，不能用作快捷键".into());
    }
    Ok(friendly(&shortcut))
}

fn prepare_shortcut(raw: &str) -> String {
    raw.split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| match part.to_ascii_lowercase().as_str() {
            "win" | "windows" | "meta" | "super" => "Super".to_string(),
            "ctrl" | "control" => "Ctrl".to_string(),
            "alt" | "option" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            _ => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

fn friendly(shortcut: &Shortcut) -> String {
    let mut parts = Vec::new();
    if shortcut.mods.contains(Modifiers::CONTROL) {
        parts.push("Ctrl".to_string());
    }
    if shortcut.mods.contains(Modifiers::ALT) {
        parts.push("Alt".to_string());
    }
    if shortcut.mods.contains(Modifiers::SHIFT) {
        parts.push("Shift".to_string());
    }
    if shortcut.mods.contains(Modifiers::SUPER) {
        parts.push("Win".to_string());
    }
    parts.push(key_label(shortcut.key));
    parts.join("+")
}

fn key_label(key: Code) -> String {
    let short = match key {
        Code::ArrowUp => "Up",
        Code::ArrowDown => "Down",
        Code::ArrowLeft => "Left",
        Code::ArrowRight => "Right",
        Code::Escape => "Esc",
        Code::KeyA => "A",
        Code::KeyB => "B",
        Code::KeyC => "C",
        Code::KeyD => "D",
        Code::KeyE => "E",
        Code::KeyF => "F",
        Code::KeyG => "G",
        Code::KeyH => "H",
        Code::KeyI => "I",
        Code::KeyJ => "J",
        Code::KeyK => "K",
        Code::KeyL => "L",
        Code::KeyM => "M",
        Code::KeyN => "N",
        Code::KeyO => "O",
        Code::KeyP => "P",
        Code::KeyQ => "Q",
        Code::KeyR => "R",
        Code::KeyS => "S",
        Code::KeyT => "T",
        Code::KeyU => "U",
        Code::KeyV => "V",
        Code::KeyW => "W",
        Code::KeyX => "X",
        Code::KeyY => "Y",
        Code::KeyZ => "Z",
        Code::Digit0 => "0",
        Code::Digit1 => "1",
        Code::Digit2 => "2",
        Code::Digit3 => "3",
        Code::Digit4 => "4",
        Code::Digit5 => "5",
        Code::Digit6 => "6",
        Code::Digit7 => "7",
        Code::Digit8 => "8",
        Code::Digit9 => "9",
        Code::F1 => "F1",
        Code::F2 => "F2",
        Code::F3 => "F3",
        Code::F4 => "F4",
        Code::F5 => "F5",
        Code::F6 => "F6",
        Code::F7 => "F7",
        Code::F8 => "F8",
        Code::F9 => "F9",
        Code::F10 => "F10",
        Code::F11 => "F11",
        Code::F12 => "F12",
        Code::Space => "Space",
        Code::Tab => "Tab",
        Code::Enter => "Enter",
        Code::Backspace => "Backspace",
        Code::Delete => "Delete",
        Code::Minus => "Minus",
        Code::Equal => "Equal",
        _ => "",
    };
    if short.is_empty() {
        key.to_string()
    } else {
        short.to_string()
    }
}

fn binds(shortcut: &Shortcut, configured: &str) -> bool {
    let Ok(wanted) = prepare_shortcut(configured).parse::<Shortcut>() else {
        return false;
    };
    shortcut.mods == wanted.mods && shortcut.key == wanted.key
}

fn sync_shortcuts(app: &AppHandle, previous: &Settings, next: &Settings) -> Result<(), String> {
    let keep = [&next.summon_shortcut, &next.capture_shortcut];
    let shortcuts = app.global_shortcut();
    for old in [&previous.summon_shortcut, &previous.capture_shortcut] {
        if keep.contains(&old) {
            continue;
        }
        if shortcuts.is_registered(old.as_str()) {
            shortcuts
                .unregister(old.as_str())
                .map_err(|err| format!("无法取消 {old}: {err}"))?;
        }
    }
    register_user_shortcuts(app, next)
}

fn unregister_user_shortcuts(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    for shortcut in [&settings.summon_shortcut, &settings.capture_shortcut] {
        if shortcuts.is_registered(shortcut.as_str()) {
            shortcuts
                .unregister(shortcut.as_str())
                .map_err(|err| format!("无法取消 {shortcut}: {err}"))?;
        }
    }
    Ok(())
}

fn apply_launch(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let autostart = app.autolaunch();
    let result = if enabled {
        autostart.enable()
    } else {
        autostart.disable()
    };
    result.map_err(|err| format!("开机自启设置失败: {err}"))
}

fn reapply_effect(app: &AppHandle, reduce_transparency: bool) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Err("缺少主窗口".into());
    };
    let effect = crate::effects::apply(&window, reduce_transparency);
    if let Some(state) = app.try_state::<std::sync::Mutex<AppState>>() {
        if let Ok(mut guard) = state.lock() {
            guard.effect = effect.clone();
        }
    }
    let _ = app.emit("effect-changed", &effect);
    Ok(())
}

fn save(settings: &Settings) -> Result<(), String> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("无法创建设置目录: {err}"))?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|err| format!("无法保存设置: {err}"))?;
    fs::write(&path, text).map_err(|err| format!("无法写入设置: {err}"))
}

fn settings_path() -> Result<PathBuf, String> {
    let appdata = std::env::var("APPDATA").map_err(|_| "找不到 APPDATA".to_string())?;
    Ok(PathBuf::from(appdata).join("Dogear").join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::{merge, normalize_shortcut, Settings, SettingsPatch};

    #[test]
    fn normalizes_modifier_order_and_case() {
        assert_eq!(normalize_shortcut("shift+alt+v").unwrap(), "Alt+Shift+V");
        assert_eq!(normalize_shortcut("win+c").unwrap(), "Win+C");
    }

    #[test]
    fn rejects_bare_keys_escape_and_duplicates() {
        assert!(normalize_shortcut("V").is_err());
        assert!(normalize_shortcut("Ctrl+Esc").is_err());
        let current = Settings::default();
        let err = merge(
            current,
            SettingsPatch {
                capture_shortcut: Some("Alt+V".into()),
                ..SettingsPatch::default()
            },
        );
        assert!(err.is_err());
    }

    #[test]
    fn missing_auto_hide_stays_off() {
        let settings: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert!(!settings.auto_hide);
        assert!(!settings.guide_seen);
        assert_eq!(settings.theme, "dark");
    }
}
