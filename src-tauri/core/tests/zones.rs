use bridge_core::displays::Rect;
use bridge_core::zones::*;

const LANDSCAPE: Rect = Rect { x: 1080, y: 0, width: 1920, height: 1040 };
const PORTRAIT: Rect = Rect { x: 0, y: 0, width: 1080, height: 1880 };

#[test]
fn every_zone_has_a_rect_inside_the_work_area() {
    for z in ZONES {
        let r = zone_rect(&LANDSCAPE, z).unwrap();
        assert!(r.x >= LANDSCAPE.x && r.y >= LANDSCAPE.y, "{z}");
        assert!(r.x + r.width as i32 <= LANDSCAPE.x + 1920 && r.y + r.height as i32 <= 1040, "{z}");
        assert_ne!(zone_label(z), "Unknown spot");
    }
    assert_eq!(zone_rect(&LANDSCAPE, "nope"), None);
}

#[test]
fn halves_and_thirds_tile_without_gaps() {
    let l = zone_rect(&LANDSCAPE, "left").unwrap();
    let r = zone_rect(&LANDSCAPE, "right").unwrap();
    assert_eq!(l, Rect { x: 1080, y: 0, width: 960, height: 1040 });
    assert_eq!(l.x + l.width as i32, r.x);
    let a = zone_rect(&PORTRAIT, "top-third").unwrap();
    let b = zone_rect(&PORTRAIT, "middle-third").unwrap();
    let c = zone_rect(&PORTRAIT, "bottom-third").unwrap();
    assert_eq!(a.y + a.height as i32, b.y);
    assert_eq!(b.y + b.height as i32, c.y);
    assert_eq!(c.y + c.height as i32, 1880);
}

#[test]
fn best_zone_recognises_snapped_windows() {
    assert_eq!(best_zone(&LANDSCAPE, &zone_rect(&LANDSCAPE, "right").unwrap()), "right");
    assert_eq!(best_zone(&LANDSCAPE, &Rect { x: 1085, y: 3, width: 1910, height: 1030 }), "full");
    assert_eq!(best_zone(&PORTRAIT, &Rect { x: 0, y: 0, width: 1080, height: 930 }), "top");
}
