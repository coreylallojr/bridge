//! Window placement for routines and workspaces: list monitors and app windows, find a step's window, and move it
//! into a zone. Plain user32/dwmapi calls on normal top-level windows (the same thing Windows Snap and FancyZones do).
use bridge_core::displays::{display_id, Rect};
use bridge_core::WindowHint;
use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetWindow, GetWindowLongW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, SetForegroundWindow, SetWindowPos, ShowWindow,
    GWL_EXSTYLE, GW_OWNER, SWP_NOACTIVATE, SWP_NOZORDER, SW_MAXIMIZE, SW_RESTORE, SW_SHOWNOACTIVATE, WS_EX_TOOLWINDOW,
};

/// A monitor in physical pixels.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    pub id: i64,
    pub bounds: Rect,
    pub work: Rect,
    pub primary: bool,
}

/// An app window the user can see (no tool windows, no cloaked UWP shells).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppWindow {
    pub hwnd: isize,
    /// Process file name, lowercase (`ms-teams.exe`). For UWP apps hosted in ApplicationFrameHost, the real app.
    pub exe: String,
    pub title: String,
    /// Visible frame (without the invisible resize borders), physical pixels.
    pub rect: Rect,
    pub monitor_id: i64,
    pub minimized: bool,
}

fn to_rect(r: &RECT) -> Rect {
    Rect { x: r.left, y: r.top, width: (r.right - r.left).max(0) as u32, height: (r.bottom - r.top).max(0) as u32 }
}

fn wide_str(buf: &[u16]) -> String {
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

fn monitor_of(hmon: HMONITOR) -> Option<Monitor> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: info is a properly sized MONITORINFOEXW.
    if !unsafe { GetMonitorInfoW(hmon, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) }.as_bool() {
        return None;
    }
    Some(Monitor {
        id: display_id(&wide_str(&info.szDevice)),
        bounds: to_rect(&info.monitorInfo.rcMonitor),
        work: to_rect(&info.monitorInfo.rcWork),
        primary: info.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
    })
}

/// All monitors, left to right (same order as the "Screen N" labels).
pub fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn cb(h: HMONITOR, _: HDC, _: *mut RECT, l: LPARAM) -> BOOL {
        let out = &mut *(l.0 as *mut Vec<Monitor>);
        if let Some(m) = monitor_of(h) {
            out.push(m);
        }
        true.into()
    }
    let mut out: Vec<Monitor> = Vec::new();
    // SAFETY: the callback only runs during this call and writes into `out`.
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut out as *mut _ as isize));
    }
    out.sort_by_key(|m| (m.bounds.x, m.bounds.y));
    out
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: plain query.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// File name of a process's image, lowercase. Needs only "limited information" access.
fn exe_of_pid(pid: u32) -> Option<String> {
    // SAFETY: handle is closed below; buffer length is passed.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return None;
        }
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        Some(full.rsplit(['\\', '/']).next().unwrap_or(&full).to_lowercase())
    }
}

/// UWP windows are hosted by ApplicationFrameHost; the real app owns a child window with another process id.
fn hosted_exe(hwnd: HWND, host_pid: u32) -> Option<String> {
    struct Find {
        host: u32,
        found: u32,
    }
    unsafe extern "system" fn cb(child: HWND, l: LPARAM) -> BOOL {
        let f = &mut *(l.0 as *mut Find);
        let pid = pid_of(child);
        if pid != 0 && pid != f.host {
            f.found = pid;
            return false.into();
        }
        true.into()
    }
    let mut f = Find { host: host_pid, found: 0 };
    // SAFETY: the callback only runs during this call.
    unsafe {
        let _ = EnumChildWindows(Some(hwnd), Some(cb), LPARAM(&mut f as *mut Find as isize));
    }
    (f.found != 0).then(|| exe_of_pid(f.found)).flatten()
}

fn frame_rect(hwnd: HWND) -> Option<RECT> {
    let mut r = RECT::default();
    // SAFETY: r is the right size for DWMWA_EXTENDED_FRAME_BOUNDS.
    unsafe {
        if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut _, std::mem::size_of::<RECT>() as u32).is_ok() {
            return Some(r);
        }
        GetWindowRect(hwnd, &mut r).ok()?;
    }
    Some(r)
}

fn describe(hwnd: HWND) -> Option<AppWindow> {
    // SAFETY: read-only queries on a window handle from EnumWindows.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return None;
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid()) {
            return None;
        }
        if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return None;
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, 4);
        if cloaked != 0 {
            return None;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        let pid = pid_of(hwnd);
        let mut exe = exe_of_pid(pid).unwrap_or_default();
        if exe == "applicationframehost.exe" {
            exe = hosted_exe(hwnd, pid).unwrap_or(exe);
        }
        if exe.is_empty() || exe == "explorer.exe" && (title == "Program Manager" || title.is_empty()) {
            return None;
        }
        let r = frame_rect(hwnd)?;
        let monitor_id = monitor_of(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)).map_or(0, |m| m.id);
        Some(AppWindow {
            hwnd: hwnd.0 as isize,
            exe,
            title,
            rect: to_rect(&r),
            monitor_id,
            minimized: IsIconic(hwnd).as_bool(),
        })
    }
}

/// Visible app windows, front to back (z-order).
pub fn windows() -> Vec<AppWindow> {
    unsafe extern "system" fn cb(h: HWND, l: LPARAM) -> BOOL {
        let out = &mut *(l.0 as *mut Vec<AppWindow>);
        if let Some(w) = describe(h) {
            out.push(w);
        }
        true.into()
    }
    let mut out: Vec<AppWindow> = Vec::new();
    // SAFETY: the callback only runs during this call.
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

/// Does this window match the hint (same app; title fragment if given)? Case-insensitive.
pub fn matches(w: &AppWindow, hint: &WindowHint) -> bool {
    w.exe == hint.exe.to_lowercase()
        && hint.title.as_ref().is_none_or(|t| t.is_empty() || w.title.to_lowercase().contains(&t.to_lowercase()))
}

/// The hint's window among `all` (its `nth` match, else the frontmost).
pub fn find<'a>(all: &'a [AppWindow], hint: &WindowHint) -> Option<&'a AppWindow> {
    all.iter().filter(|w| matches(w, hint)).nth(hint.nth.unwrap_or(0) as usize)
}

/// Moves a window so its visible frame fills `target` (physical pixels). `maximize` maximizes it on that monitor
/// instead. Restores minimized/maximized windows first. Never steals focus.
pub fn place(hwnd: isize, target: Rect, maximize: bool) -> bool {
    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: plain window positioning calls on a top-level window handle.
    unsafe {
        if IsIconic(hwnd).as_bool() || IsZoomed(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        // Twice: the first move can land on a monitor with another DPI, which changes the frame margins.
        for _ in 0..2 {
            let mut outer = RECT::default();
            if GetWindowRect(hwnd, &mut outer).is_err() {
                return false;
            }
            let frame = frame_rect(hwnd).unwrap_or(outer);
            let (l, t) = (frame.left - outer.left, frame.top - outer.top);
            let (r, b) = (outer.right - frame.right, outer.bottom - frame.bottom);
            let _ = SetWindowPos(
                hwnd,
                None,
                target.x - l,
                target.y - t,
                target.width as i32 + l + r,
                target.height as i32 + t + b,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        if maximize {
            let _ = ShowWindow(hwnd, SW_MAXIMIZE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
    true
}

/// Brings a window to the front (Windows may only flash it in the taskbar if another app has focus).
pub fn focus(hwnd: isize) {
    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: plain window calls.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}
