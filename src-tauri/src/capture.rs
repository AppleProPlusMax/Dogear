//! 区域截图。盖住整块虚拟屏幕让用户拖一块，松手后把那一块存进历史并放上剪切板。

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject,
    GetDC, GetDIBits, ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HDC, HGDIOBJ, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, ReleaseCapture, SetCapture, VK_ESCAPE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
    GetMessageW, GetSystemMetrics, GetWindowLongPtrW, GetWindowThreadProcessId, LoadCursorW,
    PostQuitMessage, RegisterClassW, SetCursor, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
    ShowWindow, TranslateMessage, UpdateLayeredWindow, GWLP_USERDATA, IDC_CROSS, MSG,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOW,
    ULW_ALPHA, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDOWN, WM_SETCURSOR,
    WM_TIMER,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::clipboard;
use crate::images;
use crate::panel;
use crate::watcher;

const MIN_EDGE: i32 = 8;
const MAX_PIXELS: i64 = 64_000_000;
const DIM: u32 = u32::from_le_bytes([0, 0, 0, 150]);
/// 设计稿里的强调色 #4a5af0，叠在选区边缘。
const EDGE: u32 = u32::from_le_bytes([0xf0, 0x5a, 0x4a, 0xff]);

static BUSY: AtomicBool = AtomicBool::new(false);

struct Shot {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

struct Overlay {
    origin_x: i32,
    origin_y: i32,
    width: i32,
    height: i32,
    pixels: *mut u32,
    mem_dc: HDC,
    dragging: bool,
    anchor: (i32, i32),
    cursor: (i32, i32),
    shot: Option<Shot>,
}

#[tauri::command]
pub fn start_capture(app: AppHandle) {
    begin(&app);
}

pub fn begin(app: &AppHandle) {
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    thread::spawn(move || {
        let visible = app
            .get_webview_window("main")
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false);
        if visible {
            panel::hide(&app);
            // 等面板从屏幕上消失，避免框选层或截图像素里带上它。
            thread::sleep(Duration::from_millis(60));
        }

        let outcome = std::panic::catch_unwind(|| unsafe { select_region() });
        match outcome {
            Ok(Ok(Some(shot))) => match grab_and_store(&app, &shot) {
                Ok(()) => panel::open(&app),
                Err(err) => {
                    eprintln!("截图保存失败: {err}");
                    if visible {
                        panel::open(&app);
                    }
                }
            },
            Ok(Ok(None)) => {
                if visible {
                    panel::open(&app);
                }
            }
            Ok(Err(err)) => {
                eprintln!("截图失败: {err}");
                if visible {
                    panel::open(&app);
                }
            }
            Err(_) => {
                eprintln!("截图中断");
                if visible {
                    panel::open(&app);
                }
            }
        }
        BUSY.store(false, Ordering::SeqCst);
    });
}

fn grab_and_store(app: &AppHandle, shot: &Shot) -> Result<(), String> {
    let bgra = unsafe { copy_screen(shot)? };
    let png = images::encode(shot.width as u32, shot.height as u32, &bgra)?;
    let hash = images::hash(&png);
    // 先入库再写入剪切板。写入时打上自己的标记，监听器就不会把同一张图再记一遍。
    watcher::record_image(app, &png, &hash);
    if let Err(err) = clipboard::write_image(&png, &hash) {
        eprintln!("截图已保存，但没能放进剪切板: {err}");
    }
    Ok(())
}

unsafe fn select_region() -> Result<Option<Shot>, String> {
    let origin_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let origin_y = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    if width <= 0 || height <= 0 {
        return Err("读不到屏幕尺寸".into());
    }
    if i64::from(width) * i64::from(height) > MAX_PIXELS {
        return Err("屏幕太大，无法框选".into());
    }

    let instance = GetModuleHandleW(None).map_err(|err| err.to_string())?;
    let class_name = w!("DogearCaptureOverlay");
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: instance.into(),
        lpszClassName: class_name,
        hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
        ..Default::default()
    };
    RegisterClassW(&window_class);

    let screen_dc = GetDC(None);
    if screen_dc.is_invalid() {
        return Err("无法读取屏幕".into());
    }
    let mem_dc = CreateCompatibleDC(Some(screen_dc));
    if mem_dc.is_invalid() {
        ReleaseDC(None, screen_dc);
        return Err("无法准备截图画布".into());
    }

    let header = dib_header(width, height);
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let dib = CreateDIBSection(
        Some(mem_dc),
        &header,
        DIB_RGB_COLORS,
        &mut bits,
        None,
        0,
    );
    if dib.is_err() || bits.is_null() {
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);
        return Err("无法创建截图画布".into());
    }
    let dib = dib.unwrap_or_default();
    let previous = SelectObject(mem_dc, HGDIOBJ(dib.0));

    let overlay = Box::new(Overlay {
        origin_x,
        origin_y,
        width,
        height,
        pixels: bits as *mut u32,
        mem_dc,
        dragging: false,
        anchor: (0, 0),
        cursor: (0, 0),
        shot: None,
    });
    let overlay = Box::into_raw(overlay);

    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        class_name,
        w!(""),
        WS_POPUP,
        origin_x,
        origin_y,
        width,
        height,
        None,
        None,
        Some(instance.into()),
        None,
    );
    let hwnd = match hwnd {
        Ok(hwnd) => hwnd,
        Err(err) => {
            let _ = Box::from_raw(overlay);
            SelectObject(mem_dc, previous);
            let _ = DeleteObject(HGDIOBJ(dib.0));
            let _ = DeleteDC(mem_dc);
            ReleaseDC(None, screen_dc);
            return Err(err.to_string());
        }
    };
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, overlay as isize);
    (*overlay).present(hwnd);
    let _ = ShowWindow(hwnd, SW_SHOW);
    take_foreground(hwnd);
    let _ = SetTimer(Some(hwnd), 1, 40, None);

    let mut message = MSG::default();
    while GetMessageW(&mut message, None, 0, 0).0 != 0 {
        let _ = TranslateMessage(&message);
        DispatchMessageW(&message);
    }

    let _ = ShowWindow(hwnd, SW_HIDE);
    let _ = DestroyWindow(hwnd);
    let mut overlay = Box::from_raw(overlay);
    let shot = overlay.shot.take();
    drop(overlay);
    SelectObject(mem_dc, previous);
    let _ = DeleteObject(HGDIOBJ(dib.0));
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
    // 等遮罩从桌面合成里消失，再去拷屏幕。
    thread::sleep(Duration::from_millis(40));
    Ok(shot)
}

impl Overlay {
    fn present(&self, hwnd: HWND) {
        paint(self);
        let dst = POINT {
            x: self.origin_x,
            y: self.origin_y,
        };
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            let _ = UpdateLayeredWindow(
                hwnd,
                None,
                Some(&dst),
                Some(&size),
                Some(self.mem_dc),
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }
}

fn paint(overlay: &Overlay) {
    let count = (overlay.width as usize) * (overlay.height as usize);
    let pixels = unsafe { std::slice::from_raw_parts_mut(overlay.pixels, count) };
    pixels.fill(DIM);
    let Some(rect) = local_rect(overlay.anchor, overlay.cursor) else {
        return;
    };
    if !overlay.dragging && overlay.shot.is_none() {
        return;
    }
    fill_rect(pixels, overlay.width, rect.0, rect.1, rect.2, rect.3, 0);
    let border = 2;
    fill_rect(
        pixels,
        overlay.width,
        rect.0 - border,
        rect.1 - border,
        rect.2 + border * 2,
        border,
        EDGE,
    );
    fill_rect(
        pixels,
        overlay.width,
        rect.0 - border,
        rect.1 + rect.3,
        rect.2 + border * 2,
        border,
        EDGE,
    );
    fill_rect(
        pixels,
        overlay.width,
        rect.0 - border,
        rect.1,
        border,
        rect.3,
        EDGE,
    );
    fill_rect(
        pixels,
        overlay.width,
        rect.0 + rect.2,
        rect.1,
        border,
        rect.3,
        EDGE,
    );
}

fn fill_rect(pixels: &mut [u32], stride: i32, x: i32, y: i32, w: i32, h: i32, color: u32) {
    if w <= 0 || h <= 0 {
        return;
    }
    let height = (pixels.len() / stride as usize) as i32;
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(stride);
    let y1 = (y + h).min(height);
    for row in y0..y1 {
        let start = (row * stride + x0) as usize;
        let end = (row * stride + x1) as usize;
        if start < end && end <= pixels.len() {
            pixels[start..end].fill(color);
        }
    }
}

/// 选区不足 8 像素时当作取消，避免误点记下一张碎图。
fn local_rect(anchor: (i32, i32), cursor: (i32, i32)) -> Option<(i32, i32, i32, i32)> {
    let x = anchor.0.min(cursor.0);
    let y = anchor.1.min(cursor.1);
    let w = (anchor.0 - cursor.0).abs();
    let h = (anchor.1 - cursor.1).abs();
    if w < 1 || h < 1 {
        return None;
    }
    Some((x, y, w, h))
}

fn accepted(anchor: (i32, i32), cursor: (i32, i32)) -> Option<(i32, i32, i32, i32)> {
    let (x, y, w, h) = local_rect(anchor, cursor)?;
    if w < MIN_EDGE || h < MIN_EDGE {
        None
    } else {
        Some((x, y, w, h))
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Overlay;
    if raw.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let overlay = &mut *raw;

    if msg == WM_SETCURSOR {
        let _ = SetCursor(Some(LoadCursorW(None, IDC_CROSS).unwrap_or_default()));
        return LRESULT(1);
    }
    if msg == WM_KEYDOWN && wparam.0 == VK_ESCAPE.0 as usize {
        finish(hwnd, overlay, None);
        return LRESULT(0);
    }
    if msg == WM_TIMER && GetAsyncKeyState(VK_ESCAPE.0 as i32) < 0 {
        finish(hwnd, overlay, None);
        return LRESULT(0);
    }
    if msg == WM_RBUTTONDOWN {
        finish(hwnd, overlay, None);
        return LRESULT(0);
    }
    if msg == WM_LBUTTONDOWN {
        let point = client_point(lparam);
        overlay.dragging = true;
        overlay.anchor = point;
        overlay.cursor = point;
        let _ = SetCapture(hwnd);
        overlay.present(hwnd);
        return LRESULT(0);
    }
    if msg == WM_MOUSEMOVE && overlay.dragging {
        overlay.cursor = client_point(lparam);
        overlay.present(hwnd);
        return LRESULT(0);
    }
    if msg == WM_LBUTTONUP && overlay.dragging {
        overlay.dragging = false;
        overlay.cursor = client_point(lparam);
        let _ = ReleaseCapture();
        let shot = accepted(overlay.anchor, overlay.cursor).and_then(|(x, y, w, h)| {
            let x = x.max(0);
            let y = y.max(0);
            if x >= overlay.width || y >= overlay.height {
                return None;
            }
            let w = w.min(overlay.width - x);
            let h = h.min(overlay.height - y);
            if w < MIN_EDGE || h < MIN_EDGE {
                return None;
            }
            Some(Shot {
                x: overlay.origin_x + x,
                y: overlay.origin_y + y,
                width: w,
                height: h,
            })
        });
        finish(hwnd, overlay, shot);
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn finish(hwnd: HWND, overlay: &mut Overlay, shot: Option<Shot>) {
    overlay.shot = shot;
    unsafe { PostQuitMessage(0) };
    let _ = hwnd;
}

fn client_point(lparam: LPARAM) -> (i32, i32) {
    let value = lparam.0 as u32;
    (value as i16 as i32, (value >> 16) as i16 as i32)
}

fn take_foreground(hwnd: HWND) {
    unsafe {
        let foreground = GetForegroundWindow();
        let their_thread = GetWindowThreadProcessId(foreground, None);
        let our_thread = GetCurrentThreadId();
        let _ = windows::Win32::System::Threading::AttachThreadInput(their_thread, our_thread, true);
        let _ = SetForegroundWindow(hwnd);
        let _ = windows::Win32::System::Threading::AttachThreadInput(their_thread, our_thread, false);
    }
}

fn dib_header(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

unsafe fn copy_screen(shot: &Shot) -> Result<Vec<u8>, String> {
    if shot.width < MIN_EDGE || shot.height < MIN_EDGE {
        return Err("选区太小".into());
    }
    if i64::from(shot.width) * i64::from(shot.height) > MAX_PIXELS {
        return Err("选区太大".into());
    }
    let screen = GetDC(None);
    if screen.is_invalid() {
        return Err("无法读取屏幕".into());
    }
    let mem = CreateCompatibleDC(Some(screen));
    if mem.is_invalid() {
        ReleaseDC(None, screen);
        return Err("无法准备截图".into());
    }
    let bitmap = CreateCompatibleBitmap(screen, shot.width, shot.height);
    if bitmap.is_invalid() {
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        return Err("无法创建截图位图".into());
    }
    let previous = SelectObject(mem, HGDIOBJ(bitmap.0));
    let copied = BitBlt(
        mem,
        0,
        0,
        shot.width,
        shot.height,
        Some(screen),
        shot.x,
        shot.y,
        SRCCOPY,
    );
    if copied.is_err() {
        SelectObject(mem, previous);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        return Err("无法复制屏幕".into());
    }

    let mut header = dib_header(shot.width, shot.height);
    let mut pixels = vec![0u8; (shot.width as usize) * (shot.height as usize) * 4];
    let lines = GetDIBits(
        mem,
        bitmap,
        0,
        shot.height as u32,
        Some(pixels.as_mut_ptr() as *mut _),
        &mut header,
        DIB_RGB_COLORS,
    );
    SelectObject(mem, previous);
    let _ = DeleteObject(HGDIOBJ(bitmap.0));
    let _ = DeleteDC(mem);
    ReleaseDC(None, screen);
    if lines == 0 {
        return Err("读不到截图像素".into());
    }
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::accepted;

    #[test]
    fn tiny_drag_is_cancelled_and_a_real_drag_keeps_its_box() {
        assert!(accepted((10, 10), (14, 30)).is_none());
        assert_eq!(accepted((40, 20), (10, 50)), Some((10, 20, 30, 30)));
    }
}
