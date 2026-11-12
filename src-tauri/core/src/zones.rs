//! Window zones on a monitor's work area: the named spots a routine step can place a window into.
use crate::displays::Rect;

/// Every zone name, in the order the layout editor shows them.
pub const ZONES: &[&str] = &[
    "full", "left", "right", "top", "bottom",
    "left-third", "center-third", "right-third", "left-two-thirds", "right-two-thirds",
    "top-third", "middle-third", "bottom-third",
    "top-left", "top-right", "bottom-left", "bottom-right",
];

/// Plain-language label for a zone ("Left half").
pub fn zone_label(zone: &str) -> &'static str {
    match zone {
        "full" => "Full screen",
        "left" => "Left half",
        "right" => "Right half",
        "top" => "Top half",
        "bottom" => "Bottom half",
        "left-third" => "Left third",
        "center-third" => "Center third",
        "right-third" => "Right third",
        "left-two-thirds" => "Left two-thirds",
        "right-two-thirds" => "Right two-thirds",
        "top-third" => "Top third",
        "middle-third" => "Middle third",
        "bottom-third" => "Bottom third",
        "top-left" => "Top-left quarter",
        "top-right" => "Top-right quarter",
        "bottom-left" => "Bottom-left quarter",
        "bottom-right" => "Bottom-right quarter",
        _ => "Unknown spot",
    }
}

/// Zone as fractions of the work area: (x, y, w, h).
fn fractions(zone: &str) -> Option<(f64, f64, f64, f64)> {
    const T: f64 = 1.0 / 3.0;
    Some(match zone {
        "full" => (0.0, 0.0, 1.0, 1.0),
        "left" => (0.0, 0.0, 0.5, 1.0),
        "right" => (0.5, 0.0, 0.5, 1.0),
        "top" => (0.0, 0.0, 1.0, 0.5),
        "bottom" => (0.0, 0.5, 1.0, 0.5),
        "left-third" => (0.0, 0.0, T, 1.0),
        "center-third" => (T, 0.0, T, 1.0),
        "right-third" => (2.0 * T, 0.0, T, 1.0),
        "left-two-thirds" => (0.0, 0.0, 2.0 * T, 1.0),
        "right-two-thirds" => (T, 0.0, 2.0 * T, 1.0),
        "top-third" => (0.0, 0.0, 1.0, T),
        "middle-third" => (0.0, T, 1.0, T),
        "bottom-third" => (0.0, 2.0 * T, 1.0, T),
        "top-left" => (0.0, 0.0, 0.5, 0.5),
        "top-right" => (0.5, 0.0, 0.5, 0.5),
        "bottom-left" => (0.0, 0.5, 0.5, 0.5),
        "bottom-right" => (0.5, 0.5, 0.5, 0.5),
        _ => return None,
    })
}

/// The zone's rectangle inside `work` (same units as `work`). Edges are rounded so neighbours share a border exactly.
pub fn zone_rect(work: &Rect, zone: &str) -> Option<Rect> {
    let (fx, fy, fw, fh) = fractions(zone)?;
    let (w, h) = (work.width as f64, work.height as f64);
    let x0 = (fx * w).round() as i32;
    let y0 = (fy * h).round() as i32;
    let x1 = ((fx + fw) * w).round() as i32;
    let y1 = ((fy + fh) * h).round() as i32;
    Some(Rect { x: work.x + x0, y: work.y + y0, width: (x1 - x0).max(0) as u32, height: (y1 - y0).max(0) as u32 })
}

fn overlap(a: &Rect, b: &Rect) -> f64 {
    let x = (a.x + a.width as i32).min(b.x + b.width as i32) - a.x.max(b.x);
    let y = (a.y + a.height as i32).min(b.y + b.height as i32) - a.y.max(b.y);
    if x <= 0 || y <= 0 { 0.0 } else { x as f64 * y as f64 }
}

/// The zone that best matches a window's current rectangle (largest intersection-over-union). Used by
/// "Capture current layout".
pub fn best_zone(work: &Rect, win: &Rect) -> &'static str {
    let area = |r: &Rect| r.width as f64 * r.height as f64;
    ZONES
        .iter()
        .filter_map(|z| {
            let r = zone_rect(work, z)?;
            let i = overlap(&r, win);
            let u = area(&r) + area(win) - i;
            Some((*z, if u > 0.0 { i / u } else { 0.0 }))
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map_or("full", |(z, _)| z)
}
