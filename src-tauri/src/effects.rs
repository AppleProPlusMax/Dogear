use serde::Serialize;
use tauri::WebviewWindow;
use tauri_utils::config::WindowEffectsConfig;
use tauri_utils::WindowEffect;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectState {
    pub effect: String,
    pub transparency_enabled: bool,
    pub high_contrast: bool,
    pub windows_build: u32,
    pub detail: String,
}

impl EffectState {
    pub fn unknown() -> Self {
        Self {
            effect: "solid".into(),
            transparency_enabled: true,
            high_contrast: false,
            windows_build: 0,
            detail: "尚未检测".into(),
        }
    }
}

#[repr(C)]
struct OsVersionInfo {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    csd: [u16; 128],
}

#[link(name = "ntdll")]
extern "system" {
    fn RtlGetVersion(info: *mut OsVersionInfo) -> i32;
}

pub fn apply(window: &WebviewWindow, reduce_transparency: bool) -> EffectState {
    let build = windows_build();
    let transparency = transparency_enabled();
    let high_contrast = high_contrast_enabled();
    let base = EffectState {
        effect: "solid".into(),
        transparency_enabled: transparency,
        high_contrast,
        windows_build: build,
        detail: String::new(),
    };

    if high_contrast {
        let _ = clear_effects(window);
        return EffectState {
            effect: "opaque".into(),
            detail: "系统高对比度模式，使用纯色".into(),
            ..base
        };
    }
    if reduce_transparency {
        let _ = clear_effects(window);
        return EffectState {
            effect: "opaque".into(),
            detail: "已开启减少透明效果，使用纯色".into(),
            ..base
        };
    }
    if !transparency {
        let _ = clear_effects(window);
        return EffectState {
            detail: "系统关闭了透明效果，使用半透明纯色".into(),
            ..base
        };
    }

    let candidates: &[(&str, WindowEffect)] = if build >= 22000 {
        &[
            ("acrylic", WindowEffect::Acrylic),
            ("mica", WindowEffect::Mica),
            ("blur", WindowEffect::Blur),
        ]
    } else if build >= 18362 {
        &[
            ("acrylic", WindowEffect::Acrylic),
            ("blur", WindowEffect::Blur),
        ]
    } else {
        &[("blur", WindowEffect::Blur)]
    };

    let mut errors = Vec::new();
    for (name, effect) in candidates {
        match apply_effect(window, *effect) {
            Ok(()) => {
                return EffectState {
                    effect: (*name).into(),
                    detail: format!("已应用 {name}（Windows build {build}）"),
                    ..base
                };
            }
            Err(err) => errors.push(format!("{name}: {err}")),
        }
    }

    let _ = clear_effects(window);
    EffectState {
        detail: format!(
            "特效都不可用，回退半透明纯色。{}",
            errors.join("；")
        ),
        ..base
    }
}

fn apply_effect(window: &WebviewWindow, effect: WindowEffect) -> Result<(), String> {
    let config = WindowEffectsConfig {
        effects: vec![effect],
        state: None,
        radius: None,
        color: None,
        interactive: false,
    };
    window
        .set_effects(config)
        .map_err(|err| err.to_string())
}

fn clear_effects(window: &WebviewWindow) -> Result<(), String> {
    window.set_effects(None).map_err(|err| err.to_string())
}

fn windows_build() -> u32 {
    unsafe {
        let mut info = OsVersionInfo {
            size: std::mem::size_of::<OsVersionInfo>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform: 0,
            csd: [0; 128],
        };
        if RtlGetVersion(&mut info) == 0 {
            info.build
        } else {
            0
        }
    }
}

fn transparency_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_value::<u32, _>("EnableTransparency"))
        .map(|value| value != 0)
        .unwrap_or(true)
}

fn high_contrast_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(r"Control Panel\Accessibility\HighContrast")
        .and_then(|key| key.get_value::<String, _>("Flags"))
        .map(|flags| flags == "1")
        .unwrap_or(false)
}
