use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HGLOBAL};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner, IsClipboardFormatAvailable,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

const CF_UNICODETEXT: u32 = 13;
const CF_BITMAP: u32 = 2;
const CF_DIB: u32 = 8;
const CF_DIBV5: u32 = 17;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

const MAX_CHARS: usize = 1_000_000;
/// 超过这个像素数就不收录，避免一张超大图把内存吃满。
const MAX_PIXELS: i64 = 64_000_000;

/// 自己写入剪切板时跳过紧接着的那一次监听，避免监听线程再把图读回来时卡住。
static SKIP_LISTENER: AtomicBool = AtomicBool::new(false);

fn skip_listener_once() {
    SKIP_LISTENER.store(true, Ordering::SeqCst);
}

fn clear_skipped_listener() {
    SKIP_LISTENER.store(false, Ordering::SeqCst);
}

pub fn consume_skipped_listener() -> bool {
    SKIP_LISTENER.swap(false, Ordering::SeqCst)
}

struct SelfWrite {
    text: String,
    at: Instant,
}

static LAST_WRITE: Mutex<Option<SelfWrite>> = Mutex::new(None);
static LAST_IMAGE: Mutex<Option<SelfWrite>> = Mutex::new(None);

pub struct ImageData {
    pub png: Vec<u8>,
    pub hash: String,
}

pub enum ClipboardRead {
    Empty,
    Ignored,
    Text(String),
    Image(ImageData),
}

/// Remember text this process just placed on the clipboard so the listener
/// does not record our own paste.
pub fn mark_self_write(text: &str) {
    if let Ok(mut guard) = LAST_WRITE.lock() {
        *guard = Some(SelfWrite {
            text: text.to_string(),
            at: Instant::now(),
        });
    }
}

pub fn is_self_write(text: &str) -> bool {
    let Ok(mut guard) = LAST_WRITE.lock() else {
        return false;
    };
    let matched = guard.as_ref().is_some_and(|write| {
        write.text == text && write.at.elapsed() < Duration::from_secs(2)
    });
    if matched {
        *guard = None;
    }
    matched
}

fn mark_self_image(hash: &str) {
    if let Ok(mut guard) = LAST_IMAGE.lock() {
        *guard = Some(SelfWrite {
            text: hash.to_string(),
            at: Instant::now(),
        });
    }
}

pub fn is_self_image(hash: &str) -> bool {
    let Ok(mut guard) = LAST_IMAGE.lock() else {
        return false;
    };
    let matched = guard.as_ref().is_some_and(|write| {
        write.text == hash && write.at.elapsed() < Duration::from_secs(4)
    });
    if matched {
        *guard = None;
    }
    matched
}

/// 复制来源的进程名，例如 `Code.exe`。剪切板所有者优先，没有再看前台窗口。
/// WebView2 的所有者是 `msedgewebview2.exe`，再往上找到宿主程序。
pub fn source_exe() -> Option<String> {
    unsafe {
        let owner = GetClipboardOwner().unwrap_or_default();
        let hwnd = if owner.0.is_null() {
            GetForegroundWindow()
        } else {
            owner
        };
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut name = process_exe(pid)?;
        let mut current = pid;
        for _ in 0..4 {
            if !name.eq_ignore_ascii_case("msedgewebview2.exe") {
                break;
            }
            let Some(parent) = parent_pid(current) else {
                break;
            };
            let Some(parent_name) = process_exe(parent) else {
                break;
            };
            current = parent;
            name = parent_name;
        }
        if name.eq_ignore_ascii_case("dogear.exe") {
            return None;
        }
        Some(name)
    }
}

unsafe fn process_exe(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buffer = vec![0u16; 1024];
    let mut len = buffer.len() as u32;
    let queried = QueryFullProcessImageNameW(
        handle,
        PROCESS_NAME_WIN32,
        windows::core::PWSTR(buffer.as_mut_ptr()),
        &mut len,
    );
    let _ = CloseHandle(handle);
    queried.ok()?;
    let path = String::from_utf16_lossy(&buffer[..len as usize]);
    std::path::Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
}

unsafe fn parent_pid(pid: u32) -> Option<u32> {
    let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
    let mut entry: PROCESSENTRY32W = std::mem::zeroed();
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    if Process32FirstW(snap, &mut entry).is_err() {
        let _ = CloseHandle(snap);
        return None;
    }
    loop {
        if entry.th32ProcessID == pid {
            let parent = entry.th32ParentProcessID;
            let _ = CloseHandle(snap);
            return Some(parent);
        }
        if Process32NextW(snap, &mut entry).is_err() {
            break;
        }
    }
    let _ = CloseHandle(snap);
    None
}

pub fn read() -> ClipboardRead {
    unsafe {
        if exclude_format_present() {
            return ClipboardRead::Ignored;
        }
        // 文字优先：带文字的复制更常见，而且图文混合时文字才是用户要的。
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok() {
            if OpenClipboard(None).is_err() {
                return ClipboardRead::Empty;
            }
            let outcome = read_unicode();
            let _ = CloseClipboard();
            return outcome;
        }
        if !image_format_present() {
            return ClipboardRead::Empty;
        }
        if OpenClipboard(None).is_err() {
            return ClipboardRead::Empty;
        }
        let outcome = read_image();
        let _ = CloseClipboard();
        outcome
    }
}

pub fn write_text(text: &str) -> Result<(), String> {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let bytes = wide.len() * std::mem::size_of::<u16>();
    skip_listener_once();
    unsafe {
        let handle = match GlobalAlloc(GMEM_MOVEABLE, bytes) {
            Ok(handle) => handle,
            Err(err) => {
                clear_skipped_listener();
                return Err(err.to_string());
            }
        };
        let locked = GlobalLock(handle);
        if locked.is_null() {
            clear_skipped_listener();
            return Err("无法锁定剪切板内存".into());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), locked as *mut u16, wide.len());
        let _ = GlobalUnlock(handle);
        if OpenClipboard(None).is_err() {
            clear_skipped_listener();
            return Err("无法打开剪切板".into());
        }
        if EmptyClipboard().is_err() {
            let _ = CloseClipboard();
            clear_skipped_listener();
            return Err("无法清空剪切板".into());
        }
        mark_self_write(text);
        let set = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(handle.0)));
        let _ = CloseClipboard();
        if let Err(err) = set {
            clear_skipped_listener();
            if let Ok(mut guard) = LAST_WRITE.lock() {
                *guard = None;
            }
            return Err(err.to_string());
        }
    }
    Ok(())
}

/// 同时放 PNG 与 CF_DIB：新软件认 PNG（保留透明），老软件认 DIB。
pub fn write_image(png: &[u8], hash: &str) -> Result<(), String> {
    let decoded = crate::images::decode(png)?;
    let dib = build_dib(decoded.width, decoded.height, &decoded.bgra)?;
    skip_listener_once();
    unsafe {
        let png_format = RegisterClipboardFormatW(w!("PNG"));
        let png_handle = match alloc_global(png) {
            Ok(handle) => handle,
            Err(err) => {
                clear_skipped_listener();
                return Err(err);
            }
        };
        let dib_handle = match alloc_global(&dib) {
            Ok(handle) => handle,
            Err(err) => {
                clear_skipped_listener();
                return Err(err);
            }
        };

        if OpenClipboard(None).is_err() {
            clear_skipped_listener();
            return Err("无法打开剪切板".into());
        }
        if EmptyClipboard().is_err() {
            let _ = CloseClipboard();
            clear_skipped_listener();
            return Err("无法清空剪切板".into());
        }
        mark_self_image(hash);
        if png_format != 0 {
            let _ = SetClipboardData(png_format, Some(HANDLE(png_handle.0)));
        }
        let set = SetClipboardData(CF_DIB, Some(HANDLE(dib_handle.0)));
        let _ = CloseClipboard();
        if let Err(err) = set {
            clear_skipped_listener();
            if let Ok(mut guard) = LAST_IMAGE.lock() {
                *guard = None;
            }
            return Err(err.to_string());
        }
    }
    Ok(())
}

unsafe fn alloc_global(bytes: &[u8]) -> Result<HGLOBAL, String> {
    let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|err| err.to_string())?;
    let locked = GlobalLock(handle);
    if locked.is_null() {
        return Err("无法锁定剪切板内存".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), locked as *mut u8, bytes.len());
    let _ = GlobalUnlock(handle);
    Ok(handle)
}

/// CF_DIB 的 alpha 通道在各家软件里表现不一致，所以先把半透明合成到白底。
fn build_dib(width: u32, height: u32, bgra: &[u8]) -> Result<Vec<u8>, String> {
    let header_size = std::mem::size_of::<BITMAPINFOHEADER>();
    let row = width as usize * 4;
    let mut out = vec![0u8; header_size + row * height as usize];

    let header = BITMAPINFOHEADER {
        biSize: header_size as u32,
        biWidth: width as i32,
        biHeight: height as i32, // 正数表示自下而上，兼容性最好
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    unsafe {
        std::ptr::copy_nonoverlapping(
            &header as *const _ as *const u8,
            out.as_mut_ptr(),
            header_size,
        );
    }

    for y in 0..height as usize {
        let src = y * row;
        let dst = header_size + (height as usize - 1 - y) * row;
        for x in (0..row).step_by(4) {
            let alpha = bgra[src + x + 3] as u32;
            for channel in 0..3 {
                let value = bgra[src + x + channel] as u32;
                let over_white = (value * alpha + 255 * (255 - alpha)) / 255;
                out[dst + x + channel] = over_white as u8;
            }
            out[dst + x + 3] = 255;
        }
    }
    Ok(out)
}

fn exclude_format_present() -> bool {
    unsafe {
        let format = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        if format == 0 {
            return false;
        }
        IsClipboardFormatAvailable(format).is_ok()
    }
}

unsafe fn image_format_present() -> bool {
    let png = RegisterClipboardFormatW(w!("PNG"));
    (png != 0 && IsClipboardFormatAvailable(png).is_ok())
        || IsClipboardFormatAvailable(CF_DIBV5).is_ok()
        || IsClipboardFormatAvailable(CF_DIB).is_ok()
        || IsClipboardFormatAvailable(CF_BITMAP).is_ok()
}

unsafe fn read_image() -> ClipboardRead {
    let png_format = RegisterClipboardFormatW(w!("PNG"));
    if png_format != 0 && IsClipboardFormatAvailable(png_format).is_ok() {
        if let Some(bytes) = read_global(png_format) {
            if bytes.starts_with(b"\x89PNG") {
                let hash = crate::images::hash(&bytes);
                return ClipboardRead::Image(ImageData { png: bytes, hash });
            }
        }
    }
    // 直接拷贝 DIB 字节。拿着剪切板调用 GetDIBits 会和界面线程互相卡住。
    for format in [CF_DIBV5, CF_DIB] {
        if IsClipboardFormatAvailable(format).is_err() {
            continue;
        }
        let Some(bytes) = read_global(format) else {
            continue;
        };
        if let Some((width, height, bgra)) = decode_dib(&bytes) {
            return image_from_bgra(width, height, &bgra);
        }
    }
    match read_bitmap() {
        Ok(Some((width, height, bgra))) => image_from_bgra(width, height, &bgra),
        Ok(None) => ClipboardRead::Ignored,
        Err(err) => {
            eprintln!("剪切板图片读取失败: {err}");
            ClipboardRead::Empty
        }
    }
}

fn image_from_bgra(width: u32, height: u32, bgra: &[u8]) -> ClipboardRead {
    match crate::images::encode(width, height, bgra) {
        Ok(png) => {
            let hash = crate::images::hash(&png);
            ClipboardRead::Image(ImageData { png, hash })
        }
        Err(err) => {
            eprintln!("剪切板图片编码失败: {err}");
            ClipboardRead::Empty
        }
    }
}

/// 把 CF_DIB / CF_DIBV5 的像素读成自上而下的 BGRA。不调用 GDI。
fn decode_dib(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.len() < 40 {
        return None;
    }
    let header = u32::from_le_bytes(bytes[0..4].try_into().ok()?) as usize;
    if header < 40 || bytes.len() < header {
        return None;
    }
    let width = i32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let height = i32::from_le_bytes(bytes[8..12].try_into().ok()?);
    let planes = u16::from_le_bytes(bytes[12..14].try_into().ok()?);
    let bit_count = u16::from_le_bytes(bytes[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(bytes[16..20].try_into().ok()?);
    if planes != 1 || width <= 0 || height == 0 || (bit_count != 24 && bit_count != 32) {
        return None;
    }
    // BI_RGB = 0，BI_BITFIELDS = 3。
    if compression != 0 && compression != 3 {
        return None;
    }
    let abs_height = height.unsigned_abs();
    let width_u = width as u32;
    if i64::from(width_u) * i64::from(abs_height) > MAX_PIXELS {
        return None;
    }
    let pixel_offset = if compression == 3 { header + 12 } else { header };
    let row_bytes = ((bit_count as usize * width as usize + 31) / 32) * 4;
    let needed = pixel_offset + row_bytes * abs_height as usize;
    if bytes.len() < needed {
        return None;
    }
    let mut bgra = vec![0u8; width as usize * abs_height as usize * 4];
    let bottom_up = height > 0;
    for y in 0..abs_height as usize {
        let src_y = if bottom_up { abs_height as usize - 1 - y } else { y };
        let src = pixel_offset + src_y * row_bytes;
        let dst = y * width as usize * 4;
        if bit_count == 32 {
            for x in 0..width as usize {
                let from = src + x * 4;
                let to = dst + x * 4;
                bgra[to..to + 3].copy_from_slice(&bytes[from..from + 3]);
                bgra[to + 3] = 255;
            }
        } else {
            for x in 0..width as usize {
                let from = src + x * 3;
                let to = dst + x * 4;
                bgra[to] = bytes[from];
                bgra[to + 1] = bytes[from + 1];
                bgra[to + 2] = bytes[from + 2];
                bgra[to + 3] = 255;
            }
        }
    }
    Some((width_u, abs_height, bgra))
}

unsafe fn read_bitmap() -> Result<Option<(u32, u32, Vec<u8>)>, String> {
    let handle = GetClipboardData(CF_BITMAP).map_err(|err| err.to_string())?;
    if handle.is_invalid() {
        return Err("剪切板没有位图".into());
    }
    let bitmap = HBITMAP(handle.0);
    let mut info = BITMAP::default();
    let read = GetObjectW(
        HGDIOBJ(bitmap.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut info as *mut _ as *mut _),
    );
    if read == 0 || info.bmWidth <= 0 || info.bmHeight == 0 {
        return Err("无法读取位图尺寸".into());
    }
    let width = info.bmWidth;
    let height = info.bmHeight.abs();
    if i64::from(width) * i64::from(height) > MAX_PIXELS {
        return Ok(None);
    }

    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // 负数表示自上而下，和 PNG 的行序一致
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let dc = CreateCompatibleDC(None);
    if dc.is_invalid() {
        return Err("无法创建设备上下文".into());
    }
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    let lines = GetDIBits(
        dc,
        bitmap,
        0,
        height as u32,
        Some(pixels.as_mut_ptr() as *mut _),
        &mut header,
        DIB_RGB_COLORS,
    );
    let _ = DeleteDC(dc);
    if lines == 0 {
        return Err("GetDIBits 返回 0 行".into());
    }

    // 来自 DIB 的 alpha 常常是 0，直接用会得到一张全透明图，所以统一按不透明处理。
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    Ok(Some((width as u32, height as u32, pixels)))
}

unsafe fn read_global(format: u32) -> Option<Vec<u8>> {
    let handle = GetClipboardData(format).ok()?;
    if handle.is_invalid() {
        return None;
    }
    let global = HGLOBAL(handle.0);
    let size = windows::Win32::System::Memory::GlobalSize(global);
    if size == 0 {
        return None;
    }
    let locked = GlobalLock(global);
    if locked.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(locked as *const u8, size).to_vec();
    let _ = GlobalUnlock(global);
    Some(bytes)
}

unsafe fn read_unicode() -> ClipboardRead {
    let Ok(handle) = GetClipboardData(CF_UNICODETEXT) else {
        return ClipboardRead::Empty;
    };
    if handle.is_invalid() {
        return ClipboardRead::Empty;
    }
    let locked = GlobalLock(windows::Win32::Foundation::HGLOBAL(handle.0));
    if locked.is_null() {
        return ClipboardRead::Empty;
    }
    let ptr = locked as *const u16;
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
        if len > MAX_CHARS {
            let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0));
            return ClipboardRead::Ignored;
        }
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    let text = String::from_utf16_lossy(slice);
    let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0));
    if text.is_empty() {
        ClipboardRead::Empty
    } else {
        ClipboardRead::Text(text)
    }
}

#[cfg(test)]
mod tests {
    use super::decode_dib;

    #[test]
    fn dib_pixels_are_read_without_gdi() {
        let mut bytes = vec![0u8; 44];
        bytes[0..4].copy_from_slice(&40u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&1i32.to_le_bytes());
        bytes[8..12].copy_from_slice(&1i32.to_le_bytes());
        bytes[12..14].copy_from_slice(&1u16.to_le_bytes());
        bytes[14..16].copy_from_slice(&32u16.to_le_bytes());
        bytes[40..44].copy_from_slice(&[1, 2, 3, 0]);
        let (width, height, bgra) = decode_dib(&bytes).unwrap();
        assert_eq!((width, height), (1, 1));
        assert_eq!(bgra, vec![1, 2, 3, 255]);
    }
}
