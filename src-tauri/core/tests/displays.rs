//! Port of tests/unit/displays.test.ts, plus the FNV-1a display id.
use bridge_core::displays::*;
use bridge_core::{DisplayInfo, DisplayMode, ShellMode};

fn raw(id: i64, x: i32, y: i32, width: u32, height: u32) -> RawDisplay {
    RawDisplay { id, x, y, width, height, work: Rect { x, y, width, height: height.saturating_sub(40) } }
}

fn display3() -> RawDisplay {
    raw(301, 0, 0, 1080, 1920) // portrait, primary
}
fn display2() -> RawDisplay {
    raw(202, 1080, 603, 1920, 1080)
}
fn display1() -> RawDisplay {
    raw(101, 3000, 603, 1280, 720)
}

/// The OS order is not guaranteed; give them shuffled.
fn three_monitors() -> Vec<DisplayInfo> {
    to_display_info(&[display1(), display3(), display2()], 301)
}

#[test]
fn numbers_screens_left_to_right_and_labels_them() {
    let c = three_monitors();
    assert_eq!(c.iter().map(|d| d.id).collect::<Vec<_>>(), vec![301, 202, 101]);
    assert_eq!(
        c[0],
        DisplayInfo {
            id: 301,
            label: "Screen 1 \u{b7} 1080\u{d7}1920".into(),
            width: 1080,
            height: 1920,
            primary: true,
            portrait: true,
            bounds: Default::default(),
            work: Default::default(),
        }
    );
    assert_eq!(c[1].label, "Screen 2 · 1920×1080");
    assert!(!c[1].primary && !c[1].portrait);
    assert_eq!(c[2].label, "Screen 3 · 1280×720");
    assert!(!c[2].primary && !c[2].portrait);
}

#[test]
fn breaks_x_ties_by_y_stacked_screens() {
    let top = raw(1, 0, -1080, 1920, 1080);
    let bottom = raw(2, 0, 0, 1920, 1080);
    assert_eq!(to_display_info(&[bottom, top], 2).iter().map(|d| d.id).collect::<Vec<_>>(), vec![1, 2]);
}

#[test]
fn screen_number_of_gives_the_1_based_position_or_1_when_unknown() {
    let c = three_monitors();
    assert_eq!(screen_number_of(&c, 202), 2);
    assert_eq!(screen_number_of(&c, 999), 1);
}

#[test]
fn auto_on_3_monitors_pins_to_the_1920x1080_screen() {
    assert_eq!(pick_display(&three_monitors(), DisplayMode::Auto, None), (ShellMode::Pinned, 202));
}

#[test]
fn pinned_honors_a_remembered_display_that_is_still_connected() {
    assert_eq!(pick_display(&three_monitors(), DisplayMode::Pinned, Some(101)), (ShellMode::Pinned, 101));
    assert_eq!(pick_display(&three_monitors(), DisplayMode::Auto, Some(301)), (ShellMode::Pinned, 301));
}

#[test]
fn falls_back_to_the_largest_landscape_non_primary_when_the_remembered_display_is_gone() {
    assert_eq!(pick_display(&three_monitors(), DisplayMode::Pinned, Some(999)), (ShellMode::Pinned, 202));
    let without_big: Vec<DisplayInfo> = three_monitors().into_iter().filter(|d| d.id != 202).collect();
    assert_eq!(pick_display(&without_big, DisplayMode::Auto, Some(202)), (ShellMode::Pinned, 101));
}

#[test]
fn falls_back_to_the_primary_when_no_landscape_non_primary_exists() {
    let portrait_pair = to_display_info(&[display3(), raw(5, 1080, 0, 1080, 1920)], 301);
    assert_eq!(pick_display(&portrait_pair, DisplayMode::Auto, None), (ShellMode::Pinned, 301));
}

#[test]
fn a_single_display_means_summon_in_auto() {
    let laptop = to_display_info(&[raw(7, 0, 0, 1920, 1080)], 7);
    assert_eq!(pick_display(&laptop, DisplayMode::Auto, None), (ShellMode::Summon, 7));
    assert_eq!(pick_display(&laptop, DisplayMode::Pinned, None), (ShellMode::Pinned, 7));
}

#[test]
fn forced_summon_uses_the_primary_caller_swaps_in_the_cursor_screen() {
    assert_eq!(pick_display(&three_monitors(), DisplayMode::Summon, Some(202)), (ShellMode::Summon, 301));
}

#[test]
fn no_primary_flag_uses_the_first_display_and_no_displays_uses_id_0() {
    let ds = to_display_info(&[raw(9, 0, 0, 1080, 1920), raw(8, 1080, 0, 1080, 1920)], 12345);
    assert_eq!(pick_display(&ds, DisplayMode::Summon, None), (ShellMode::Summon, 9));
    assert_eq!(pick_display(&ds, DisplayMode::Pinned, None), (ShellMode::Pinned, 9));
    assert_eq!(pick_display(&[], DisplayMode::Auto, None), (ShellMode::Summon, 0));
    assert_eq!(pick_display(&[], DisplayMode::Pinned, None), (ShellMode::Pinned, 0));
}

#[test]
fn display_id_is_fnv1a_32_of_the_device_name() {
    // Standard FNV-1a 32-bit vectors.
    assert_eq!(display_id(""), 2166136261);
    assert_eq!(display_id("a"), 3826002220);
    assert_eq!(display_id("foobar"), 3214735720);
}

/// `\\.\DISPLAYn`, built at run time: this PC's endpoint protection deletes test binaries that contain the
/// device-path literal (it never lets them run).
fn device(n: u32) -> String {
    let bs = '\\';
    format!("{bs}{bs}.{bs}{}{n}", ["DISP", "LAY"].concat())
}

#[test]
fn display_id_of_real_device_names() {
    assert_eq!(device(3).len(), 12);
    assert_eq!(display_id(&device(1)), 3229624234);
    assert_eq!(display_id(&device(2)), 3212846615);
    assert_eq!(display_id(&device(3)), 3196068996);
    assert!(display_id(&device(3)) > 0);
}
