//! 手动应用内检查更新。仅在用户点击设置页「检查更新」时通过 WinHTTP 查询 GitHub Releases。

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use windows::core::{w, PCWSTR};
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
    WINHTTP_FLAG_SECURE, WINHTTP_FLAG_SECURE_DEFAULTS, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE,
};

use crate::state::AppState;

const RELEASES_API_HOST: &str = "api.github.com";
const RELEASES_API_PATH: &str = "/repos/AppleProPlusMax/Dogear/releases/latest";
const RELEASES_PAGE_URL: &str = "https://github.com/AppleProPlusMax/Dogear/releases";
const ALLOWED_URL_PREFIX: &str = "https://github.com/AppleProPlusMax/Dogear/releases";
const HTTPS_PORT: u16 = 443;
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_NOTES_CHARS: usize = 800;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_url: String,
    pub download_url: Option<String>,
    pub notes: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

struct HttpHandle(*mut core::ffi::c_void);

impl HttpHandle {
    fn new(raw: *mut core::ffi::c_void) -> Option<Self> {
        if raw.is_null() {
            None
        } else {
            Some(Self(raw))
        }
    }
}

impl Drop for HttpHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let lang = current_language(&app);
    let task_lang = lang.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let current = env!("CARGO_PKG_VERSION");
        let body = fetch_latest_release(&task_lang)?;
        parse_release(current, &body, &task_lang)
    })
    .await
    .map_err(|_| say(&lang, "检查更新被中断", "Update check was interrupted"))?
}

#[tauri::command]
pub async fn open_release_url(app: AppHandle, url: String) -> Result<(), String> {
    let lang = current_language(&app);
    if !is_allowed_release_url(&url) {
        return Err(say(
            &lang,
            "只能打开 Dogear 官方发布页链接",
            "Only official Dogear release links can be opened",
        ));
    }
    tauri::async_runtime::spawn_blocking(move || crate::paste::open_in_browser(&url))
        .await
        .map_err(|_| say(&lang, "打开页面被中断", "Opening release page was interrupted"))?
}

fn current_language(app: &AppHandle) -> String {
    app.try_state::<Mutex<AppState>>()
        .and_then(|state| state.lock().ok().map(|guard| guard.settings.language.clone()))
        .unwrap_or_else(|| "zh-CN".into())
}

fn say(lang: &str, zh: &str, en: &str) -> String {
    if lang == "en" {
        en.to_string()
    } else {
        zh.to_string()
    }
}

fn wide_null(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn fetch_latest_release(lang: &str) -> Result<Vec<u8>, String> {
    let agent = wide_null(&format!("Dogear/{}", env!("CARGO_PKG_VERSION")));
    let host = wide_null(RELEASES_API_HOST);
    let path = wide_null(RELEASES_API_PATH);
    let headers: Vec<u16> =
        "Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n"
            .encode_utf16()
            .collect();

    unsafe {
        let session = HttpHandle::new(WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            WINHTTP_FLAG_SECURE_DEFAULTS,
        ))
        .or_else(|| {
            HttpHandle::new(WinHttpOpen(
                PCWSTR(agent.as_ptr()),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ))
        })
        .or_else(|| {
            HttpHandle::new(WinHttpOpen(
                PCWSTR(agent.as_ptr()),
                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ))
        })
        .ok_or_else(|| say(lang, "无法初始化网络会话", "Could not initialize network session"))?;

        let _ = WinHttpSetTimeouts(session.0, 8_000, 8_000, 10_000, 10_000);

        let connect = HttpHandle::new(WinHttpConnect(
            session.0,
            PCWSTR(host.as_ptr()),
            HTTPS_PORT,
            0,
        ))
        .ok_or_else(|| say(lang, "无法连接到 GitHub", "Could not connect to GitHub"))?;

        let request = HttpHandle::new(WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))
        .ok_or_else(|| say(lang, "无法创建更新请求", "Could not create update request"))?;

        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0).map_err(|_| {
            say(
                lang,
                "连接 GitHub 失败，请检查网络或代理设置",
                "Could not reach GitHub. Check your network or proxy",
            )
        })?;

        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|_| {
            say(
                lang,
                "等待 GitHub 响应超时，请稍后再试",
                "Timed out waiting for GitHub. Try again later",
            )
        })?;

        let mut status: u32 = 0;
        let mut status_len = std::mem::size_of::<u32>() as u32;
        let mut header_index = 0u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut core::ffi::c_void),
            &mut status_len,
            &mut header_index,
        )
        .map_err(|_| say(lang, "无法读取响应状态", "Could not read response status"))?;

        if status == 403 || status == 429 {
            return Err(say(
                lang,
                "GitHub 接口请求过于频繁，请稍后再试",
                "GitHub API rate limit reached. Try again later",
            ));
        }
        if status == 404 {
            return Err(say(
                lang,
                "暂未找到已发布的版本",
                "No published release found yet",
            ));
        }
        if status != 200 {
            return Err(if lang == "en" {
                format!("Update check failed (HTTP {status})")
            } else {
                format!("检查更新失败（HTTP {status}）")
            });
        }

        let mut body = Vec::new();
        loop {
            let mut available = 0u32;
            WinHttpQueryDataAvailable(request.0, &mut available)
                .map_err(|_| say(lang, "读取更新信息失败", "Failed to read update response"))?;
            if available == 0 {
                break;
            }
            let remaining = MAX_RESPONSE_BYTES.saturating_sub(body.len());
            if remaining == 0 {
                break;
            }
            let to_read = (available as usize).min(remaining).min(16 * 1024);
            let mut chunk = vec![0u8; to_read];
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr() as *mut core::ffi::c_void,
                to_read as u32,
                &mut read,
            )
            .map_err(|_| say(lang, "读取更新信息失败", "Failed to read update response"))?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read as usize]);
        }
        Ok(body)
    }
}

fn parse_release(current: &str, body: &[u8], lang: &str) -> Result<UpdateInfo, String> {
    let release: GithubRelease = serde_json::from_slice(body).map_err(|_| {
        say(
            lang,
            "无法解析 GitHub 发布信息",
            "Could not parse GitHub release data",
        )
    })?;
    let current_clean = strip_v(current);
    let latest_clean = strip_v(&release.tag_name);
    if latest_clean.is_empty() {
        return Err(say(
            lang,
            "发布信息缺少版本号",
            "Release data is missing a version tag",
        ));
    }
    let has_update = is_newer(current_clean, latest_clean);
    let release_url = if is_allowed_release_url(&release.html_url) {
        release.html_url
    } else {
        RELEASES_PAGE_URL.to_string()
    };
    let download_url = pick_download_url(&release.assets);
    let notes = trim_notes(&release.body);
    Ok(UpdateInfo {
        current_version: current_clean.to_string(),
        latest_version: latest_clean.to_string(),
        has_update,
        release_url,
        download_url,
        notes,
    })
}

fn pick_download_url(assets: &[GithubAsset]) -> Option<String> {
    let msi_zh = assets.iter().find(|asset| {
        let lower = asset.name.to_ascii_lowercase();
        lower.ends_with(".msi")
            && lower.contains("zh-cn")
            && is_allowed_release_url(&asset.browser_download_url)
    });
    if let Some(asset) = msi_zh {
        return Some(asset.browser_download_url.clone());
    }
    let msi = assets.iter().find(|asset| {
        asset.name.to_ascii_lowercase().ends_with(".msi")
            && is_allowed_release_url(&asset.browser_download_url)
    });
    if let Some(asset) = msi {
        return Some(asset.browser_download_url.clone());
    }
    assets
        .iter()
        .find(|asset| {
            asset.name.to_ascii_lowercase().ends_with(".exe")
                && is_allowed_release_url(&asset.browser_download_url)
        })
        .map(|asset| asset.browser_download_url.clone())
}

fn trim_notes(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.chars().count() <= MAX_NOTES_CHARS {
        return trimmed.to_string();
    }
    let prefix: String = trimmed.chars().take(MAX_NOTES_CHARS).collect();
    format!("{}…", prefix.trim_end())
}

fn strip_v(version: &str) -> &str {
    version.trim().trim_start_matches(['v', 'V'])
}

fn parse_semver(version: &str) -> Option<(u64, u64, u64, bool)> {
    let cleaned = strip_v(version);
    if cleaned.is_empty() {
        return None;
    }
    let no_build = cleaned.split('+').next()?;
    let (core, prerelease) = match no_build.split_once('-') {
        Some((c, _)) => (c, true),
        None => (no_build, false),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, prerelease))
}

fn is_newer(current: &str, latest: &str) -> bool {
    let (Some(cur), Some(lat)) = (parse_semver(current), parse_semver(latest)) else {
        return false;
    };
    match (lat.0, lat.1, lat.2).cmp(&(cur.0, cur.1, cur.2)) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => cur.3 && !lat.3,
    }
}

fn is_allowed_release_url(url: &str) -> bool {
    let trimmed = url.trim();
    trimmed == ALLOWED_URL_PREFIX
        || trimmed
            .strip_prefix(ALLOWED_URL_PREFIX)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::{is_allowed_release_url, is_newer, parse_release, RELEASES_PAGE_URL};

    #[test]
    fn semver_comparison_handles_prefixes_and_prereleases() {
        assert!(is_newer("0.1.9", "v0.1.10"));
        assert!(is_newer("0.1.9", "0.2.0"));
        assert!(is_newer("0.1.9", "1.0.0"));
        assert!(!is_newer("0.1.9", "v0.1.9"));
        assert!(!is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("0.2.0-beta.1", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0-beta.1"));
        assert!(!is_newer("0.1.9", "invalid"));
    }

    #[test]
    fn release_parser_prefers_zh_msi_and_filters_untrusted_urls() {
        let json = br#"{
            "tag_name": "v0.2.0",
            "html_url": "https://github.com/AppleProPlusMax/Dogear/releases/tag/v0.2.0",
            "body": "  New features and fixes.  ",
            "assets": [
                {
                    "name": "Dogear_0.2.0_x64-setup.exe",
                    "browser_download_url": "https://github.com/AppleProPlusMax/Dogear/releases/download/v0.2.0/Dogear_0.2.0_x64-setup.exe"
                },
                {
                    "name": "Dogear_0.2.0_x64_zh-CN.msi",
                    "browser_download_url": "https://github.com/AppleProPlusMax/Dogear/releases/download/v0.2.0/Dogear_0.2.0_x64_zh-CN.msi"
                }
            ]
        }"#;
        let info = parse_release("0.1.9", json, "zh-CN").unwrap();
        assert!(info.has_update);
        assert_eq!(info.current_version, "0.1.9");
        assert_eq!(info.latest_version, "0.2.0");
        assert_eq!(
            info.download_url.as_deref(),
            Some("https://github.com/AppleProPlusMax/Dogear/releases/download/v0.2.0/Dogear_0.2.0_x64_zh-CN.msi")
        );
        assert_eq!(info.notes, "New features and fixes.");

        let untrusted = br#"{
            "tag_name": "v0.1.9",
            "html_url": "https://evil.example/phish",
            "body": "",
            "assets": [
                {
                    "name": "Dogear_0.1.9_x64_zh-CN.msi",
                    "browser_download_url": "https://evil.example/Dogear.msi"
                }
            ]
        }"#;
        let safe = parse_release("0.1.9", untrusted, "en").unwrap();
        assert!(!safe.has_update);
        assert_eq!(safe.release_url, RELEASES_PAGE_URL);
        assert_eq!(safe.download_url, None);
    }

    #[test]
    fn allowlist_only_permits_official_release_urls() {
        assert!(is_allowed_release_url(
            "https://github.com/AppleProPlusMax/Dogear/releases"
        ));
        assert!(is_allowed_release_url(
            "https://github.com/AppleProPlusMax/Dogear/releases/tag/v0.1.9"
        ));
        assert!(!is_allowed_release_url(
            "https://github.com/AppleProPlusMax/Dogear/releases.evil.com/x"
        ));
        assert!(!is_allowed_release_url("http://github.com/AppleProPlusMax/Dogear/releases"));
        assert!(!is_allowed_release_url("file:///C:/Windows/System32/calc.exe"));
    }
}
