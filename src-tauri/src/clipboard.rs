use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::core::w;
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};

const CF_UNICODETEXT: u32 = 13;
const CF_BITMAP: u32 = 2;
const CF_DIB: u32 = 8;
const CF_DIBV5: u32 = 17;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

const MAX_CHARS: usize = 1_000_000;
/// 超过这个像素数就不收录，避免一张超大图把内存吃满。
const MAX_PIXELS: i64 = 64_000_000;

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
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|err| err.to_string())?;
        let locked = GlobalLock(handle);
        if locked.is_null() {
            return Err("无法锁定剪切板内存".into());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), locked as *mut u16, wide.len());
        let _ = GlobalUnlock(handle);
        OpenClipboard(None).map_err(|err| err.to_string())?;
        if EmptyClipboard().is_err() {
            let _ = CloseClipboard();
            return Err("无法清空剪切板".into());
        }
        mark_self_write(text);
        let set = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(handle.0)));
        let _ = CloseClipboard();
        set.map_err(|err| {
            if let Ok(mut guard) = LAST_WRITE.lock() {
                *guard = None;
            }
            err.to_string()
        })?;
    }
    Ok(())
}

/// 同时放 PNG 与 CF_DIB：新软件认 PNG（保留透明），老软件认 DIB。
pub fn write_image(png: &[u8], hash: &str) -> Result<(), String> {
    let decoded = crate::images::decode(png)?;
    let dib = build_dib(decoded.width, decoded.height, &decoded.bgra)?;
    unsafe {
        let png_format = RegisterClipboardFormatW(w!("PNG"));
        let png_handle = alloc_global(png)?;
        let dib_handle = alloc_global(&dib)?;

        OpenClipboard(None).map_err(|err| err.to_string())?;
        if EmptyClipboard().is_err() {
            let _ = CloseClipboard();
            return Err("无法清空剪切板".into());
        }
        mark_self_image(hash);
        if png_format != 0 {
            let _ = SetClipboardData(png_format, Some(HANDLE(png_handle.0)));
        }
        let set = SetClipboardData(CF_DIB, Some(HANDLE(dib_handle.0)));
        let _ = CloseClipboard();
        set.map_err(|err| {
            if let Ok(mut guard) = LAST_IMAGE.lock() {
                *guard = None;
            }
            err.to_string()
        })?;
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
    // 走 GDI：无论源是 DIB、DIBV5 还是位图，都统一拿到 32 位 BGRA。
    match read_bitmap() {
        Ok(Some((width, height, bgra))) => match crate::images::encode(width, height, &bgra) {
            Ok(png) => {
                let hash = crate::images::hash(&png);
                ClipboardRead::Image(ImageData { png, hash })
            }
            Err(err) => {
                eprintln!("剪切板图片编码失败: {err}");
                ClipboardRead::Empty
            }
        },
        Ok(None) => ClipboardRead::Ignored,
        Err(err) => {
            eprintln!("剪切板图片读取失败: {err}");
            ClipboardRead::Empty
        }
    }
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
