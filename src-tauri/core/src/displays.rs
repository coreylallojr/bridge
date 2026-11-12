//! Pure display logic. Port of `src/main/shell/displays.ts`; the OS enumeration lives in bridge-shell.
use crate::types::{DisplayInfo, DisplayMode, ShellMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// One monitor as the OS reports it. Bounds are in DIPs; `work` is the work area (DIPs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RawDisplay {
    /// [`display_id`] of the device name.
    pub id: i64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub work: Rect,
}

/// Stable display id: FNV-1a 32-bit of the monitor device name (e.g. `\\.\DISPLAY3`), always non-negative.
pub fn display_id(device_name: &str) -> i64 {
    let mut h: u32 = 0x811c_9dc5;
    for b in device_name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h as i64
}

/// Screens numbered by physical position: left to right, then top to bottom.
/// Label: `Screen N · W×H` (bounds size in DIPs).
pub fn to_display_info(raw: &[RawDisplay], primary: i64) -> Vec<DisplayInfo> {
    let mut ds = raw.to_vec();
    ds.sort_by_key(|d| (d.x, d.y));
    ds.iter()
        .enumerate()
        .map(|(i, d)| DisplayInfo {
            id: d.id,
            label: format!("Screen {} \u{b7} {}\u{d7}{}", i + 1, d.width, d.height),
            width: d.width,
            height: d.height,
            primary: d.id == primary,
            portrait: d.height > d.width,
            // Filled in by the app from the physical monitor list (for drawing the layout map to scale).
            bounds: Rect::default(),
            work: Rect::default(),
        })
        .collect()
}

/// 1-based screen number of a display in a `to_display_info` list (1 if it's not there).
pub fn screen_number_of(displays: &[DisplayInfo], id: i64) -> u32 {
    displays.iter().position(|d| d.id == id).map_or(1, |i| i as u32 + 1)
}

fn primary_of(displays: &[DisplayInfo]) -> Option<&DisplayInfo> {
    displays.iter().find(|d| d.primary).or(displays.first())
}

/// Which mode and screen Bridge should use.
/// - auto = pinned with 2+ displays, else summon.
/// - pinned target = the remembered display if still connected, else the largest landscape non-primary, else the
///   primary.
/// - summon returns the primary; the window manager swaps in the cursor's screen when it shows.
pub fn pick_display(ds: &[DisplayInfo], mode: DisplayMode, pinned: Option<i64>) -> (ShellMode, i64) {
    let primary = primary_of(ds);
    let primary_id = primary.map_or(0, |d| d.id);
    let effective = match mode {
        DisplayMode::Auto if ds.len() >= 2 => ShellMode::Pinned,
        DisplayMode::Auto | DisplayMode::Summon => ShellMode::Summon,
        DisplayMode::Pinned => ShellMode::Pinned,
    };
    if effective == ShellMode::Summon {
        return (ShellMode::Summon, primary_id);
    }
    if let Some(id) = pinned.filter(|id| ds.iter().any(|d| d.id == *id)) {
        return (ShellMode::Pinned, id);
    }
    let mut best: Option<&DisplayInfo> = None;
    for d in ds {
        if d.primary || d.portrait {
            continue;
        }
        if best.map_or(true, |b| d.width as u64 * d.height as u64 > b.width as u64 * b.height as u64) {
            best = Some(d);
        }
    }
    (ShellMode::Pinned, best.or(primary).map_or(primary_id, |d| d.id))
}
