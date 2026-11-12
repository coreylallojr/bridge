//! Raw Win32 window placement for the main window. These calls are thread-agnostic (Windows marshals them to the
//! window's thread), so they never wait on Tauri's event loop.
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetTopWindow, GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextLengthW, IsIconic,
    IsWindowVisible, SetForegroundWindow, SetWindowPos, ShowWindow, GWL_EXSTYLE, GW_HWNDNEXT, HWND_BOTTOM, HWND_TOP,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_SHOWNOACTIVATE, WS_EX_TOOLWINDOW,
};

/// Pinned mode on blur: drop to the bottom of the z-order (a live-wallpaper layer). Never top-most, never activated.
pub fn send_to_bottom(hwnd: HWND) {
    if let Err(e) = unsafe { SetWindowPos(hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) }
    {
        eprintln!("[bridge] sendToBottom failed: {e}");
    }
}

/// Electron's `moveTop`: raise above other normal windows even if Windows refuses to hand over focus.
pub fn move_top(hwnd: HWND) {
    let _ = unsafe { SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
}

/// Electron's `showInactive`.
pub fn show_inactive(hwnd: HWND) {
    let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
}

pub fn is_visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

pub fn is_minimized(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd).as_bool() }
}

pub fn is_focused(hwnd: HWND) -> bool {
    unsafe { GetForegroundWindow() == hwnd }
}

/// Outer bounds in physical pixels: (x, y, width, height).
pub fn bounds(hwnd: HWND) -> Option<(i32, i32, u32, u32)> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r) }.ok()?;
    Some((r.left, r.top, (r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32))
}

/// Outer bounds in physical pixels, z-order untouched, not activated.
pub fn set_bounds(hwnd: HWND, x: i32, y: i32, w: u32, h: u32) {
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, w as i32, h as i32, SWP_NOZORDER | SWP_NOACTIVATE) };
}

/// Electron's `blur()`: hand activation to the next real window in the z-order.
pub fn deactivate(hwnd: HWND) {
    unsafe {
        let mut cur = GetTopWindow(None).ok();
        while let Some(w) = cur {
            if w != hwnd
                && IsWindowVisible(w).as_bool()
                && !IsIconic(w).as_bool()
                && GetWindowLongW(w, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 == 0
                && GetWindowTextLengthW(w) > 0
            {
                let _ = SetForegroundWindow(w);
                return;
            }
            cur = GetWindow(w, GW_HWNDNEXT).ok();
        }
    }
}
