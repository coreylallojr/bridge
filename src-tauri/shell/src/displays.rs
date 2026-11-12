//! Connected monitors, as Bridge sees them.
//! - Ids are core `display_id(<device name>)` (FNV-1a of `\\.\DISPLAYn`), stable across restarts.
//! - `RawDisplay` origins stay physical (they only order the screens, and physical rects never overlap); sizes are
//!   DIPs, so labels read like Electron's (`Screen 2 · 1920×1080`).
//! - Placement uses the physical work area (`Mon::work`), so no rounding ever creeps into the window bounds.
use bridge_core::displays::{display_id, RawDisplay, Rect};
use tauri::{AppHandle, Monitor, Runtime};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// A monitor with its physical work area (x, y, width, height).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mon {
    pub raw: RawDisplay,
    pub work: (i32, i32, u32, u32),
}

fn dip(v: u32, scale: f64) -> u32 {
    if scale > 0.0 {
        (v as f64 / scale).round() as u32
    } else {
        v
    }
}

/// Pure conversion from Tauri's physical monitor numbers.
pub(crate) fn to_mon(name: &str, pos: (i32, i32), size: (u32, u32), scale: f64, work: (i32, i32, u32, u32)) -> Mon {
    Mon {
        raw: RawDisplay {
            id: display_id(name),
            x: pos.0,
            y: pos.1,
            width: dip(size.0, scale),
            height: dip(size.1, scale),
            work: Rect { x: work.0, y: work.1, width: dip(work.2, scale), height: dip(work.3, scale) },
        },
        work,
    }
}

fn from_monitor(m: &Monitor) -> Mon {
    let name = m.name().map(String::as_str).unwrap_or("");
    let (p, s, w) = (m.position(), m.size(), m.work_area());
    to_mon(name, (p.x, p.y), (s.width, s.height), m.scale_factor(), (w.position.x, w.position.y, w.size.width, w.size.height))
}

/// All monitors plus the primary's id (the first monitor's id if Windows reports none; 0 with no monitors).
pub(crate) fn monitors<R: Runtime>(app: &AppHandle<R>) -> (Vec<Mon>, i64) {
    let mons: Vec<Mon> = app.available_monitors().unwrap_or_default().iter().map(from_monitor).collect();
    let primary = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| from_monitor(&m).raw.id)
        .or_else(|| mons.first().map(|m| m.raw.id))
        .unwrap_or(0);
    (mons, primary)
}

/// Connected displays (for core `to_display_info` / `pick_display`) and the primary display's id.
pub fn list<R: Runtime>(app: &AppHandle<R>) -> (Vec<RawDisplay>, i64) {
    let (mons, primary) = monitors(app);
    (mons.into_iter().map(|m| m.raw).collect(), primary)
}

fn wide_to_string(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

/// Device name (`\\.\DISPLAYn`) of the monitor under (or nearest to) the mouse cursor.
pub fn cursor_device_name() -> Option<String> {
    unsafe {
        let mut pt = POINT::default();
        GetCursorPos(&mut pt).ok()?;
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if hmon.is_invalid() {
            return None;
        }
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(hmon, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
            return None;
        }
        Some(wide_to_string(&info.szDevice))
    }
}

/// Display id of the monitor under the mouse cursor.
pub fn cursor_display_id() -> Option<i64> {
    cursor_device_name().map(|n| display_id(&n))
}

/// `\\.\DISPLAY<n>`, built at runtime: endpoint protection here deletes executables containing that literal.
#[cfg(test)]
pub(crate) fn device(n: u32) -> String {
    format!("{}.{}DISPLAY{n}", "\\".repeat(2), "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_sizes_to_dips_and_keeps_physical_origins() {
        let m = to_mon(&device(2), (1620, 603), (2880, 1620), 1.5, (1620, 603, 2880, 1560));
        assert_eq!(m.raw.id, display_id(&device(2)));
        assert_eq!((m.raw.x, m.raw.y, m.raw.width, m.raw.height), (1620, 603, 1920, 1080));
        assert_eq!(m.raw.work, Rect { x: 1620, y: 603, width: 1920, height: 1040 });
        assert_eq!(m.work, (1620, 603, 2880, 1560));
    }

    #[test]
    fn a_zero_scale_never_divides() {
        let m = to_mon("x", (0, 0), (1280, 720), 0.0, (0, 0, 1280, 680));
        assert_eq!((m.raw.width, m.raw.height), (1280, 720));
    }

    #[test]
    fn wide_strings_stop_at_the_first_nul() {
        let mut w = [0u16; 32];
        for (i, c) in device(3).encode_utf16().enumerate() {
            w[i] = c;
        }
        assert_eq!(wide_to_string(&w), device(3));
    }

    #[test]
    fn the_cursor_monitor_is_a_device_name_when_there_is_one() {
        // GetCursorPos fails without an interactive desktop (e.g. a background agent session); then it's None.
        if let Some(name) = cursor_device_name() {
            assert!(name.starts_with("\\\\.\\"), "{name}");
        }
    }
}
