use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::core::w;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};

const CF_UNICODETEXT: u32 = 13;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

const MAX_CHARS: usize = 1_000_000;

struct SelfWrite {
    text: String,
    at: Instant,
}

static LAST_WRITE: Mutex<Option<SelfWrite>> = Mutex::new(None);

pub enum ClipboardRead {
    Empty,
    Ignored,
    Text(String),
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

pub fn read() -> ClipboardRead {
    unsafe {
        if exclude_format_present() {
            return ClipboardRead::Ignored;
        }
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
            return ClipboardRead::Empty;
        }
        if OpenClipboard(None).is_err() {
            return ClipboardRead::Empty;
        }
        let outcome = read_unicode();
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

fn exclude_format_present() -> bool {
    unsafe {
        let format = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        if format == 0 {
            return false;
        }
        IsClipboardFormatAvailable(format).is_ok()
    }
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
