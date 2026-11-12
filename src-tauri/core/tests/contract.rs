//! The Rust structs must round-trip the real profile files and speak the renderer's event shapes.
use bridge_core::*;
use serde_json::{json, Value};

fn roundtrip(path: &str) {
    let raw = std::fs::read_to_string(format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let raw = raw.trim_start_matches('\u{feff}');
    let original: Value = serde_json::from_str(raw).unwrap();
    let parsed: Profile = serde_json::from_value(original.clone()).unwrap();
    let back = serde_json::to_value(&parsed).unwrap();
    assert_eq!(back, original, "{path} must round-trip unchanged");
}

#[test]
fn demo_profile_roundtrips() {
    roundtrip("tests/fixtures/demo-profile.json");
}

#[test]
fn starter_profile_roundtrips() {
    roundtrip("examples/starter-profile.json");
}

#[test]
fn command_event_shape() {
    let e = BridgeEvent::Command { command: UiCommand::FocusPalette };
    assert_eq!(serde_json::to_value(&e).unwrap(), json!({"type":"command","command":"focus-palette"}));
}

#[test]
fn launched_event_uses_camel_case() {
    let e = BridgeEvent::Launched { tile_id: "t-1".into(), ok: true, message: "Opened X".into() };
    assert_eq!(
        serde_json::to_value(&e).unwrap(),
        json!({"type":"launched","tileId":"t-1","ok":true,"message":"Opened X"})
    );
}

#[test]
fn res_omits_missing_data() {
    assert_eq!(serde_json::to_value(Res::<()>::fail("nope")).unwrap(), json!({"ok":false,"message":"nope"}));
}

#[test]
fn settings_patch_distinguishes_null_from_missing() {
    let p: SettingsPatch = serde_json::from_value(json!({"pinnedDisplayId": null})).unwrap();
    assert_eq!(p.pinned_display_id, Some(None));
    let p: SettingsPatch = serde_json::from_value(json!({"mode": "pinned"})).unwrap();
    assert_eq!(p.pinned_display_id, None);
    assert_eq!(p.mode, Some(DisplayMode::Pinned));
}
