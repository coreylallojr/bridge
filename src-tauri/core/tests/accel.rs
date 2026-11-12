//! Electron accelerator (as saved by SettingsDrawer.tsx `toAccelerator`) -> `global-hotkey` 0.8 string.
//!
//! Outputs are in global-hotkey's canonical form (`HotKey::into_string`: `shift+control+alt+super+<Code>`), so for
//! every `Some(s)` below `s.parse::<global_hotkey::hotkey::HotKey>().unwrap().to_string() == s`.
//! bridge-core does not depend on global-hotkey; bridge-shell should repeat this table with that round-trip check:
//!
//! ```ignore
//! for (accel, want) in TABLE { let s = accel_to_shortcut(accel).unwrap();
//!     assert_eq!(s.parse::<HotKey>().unwrap().to_string(), s); }
//! ```
use bridge_core::accel::accel_to_shortcut;

#[test]
fn review_focus_table_maps_old_profile_hotkeys() {
    let table = [
        ("Alt+Space", "alt+Space"),
        ("Ctrl+Shift+F12", "shift+control+F12"),
        ("Super+num5", "super+Numpad5"),
        ("Ctrl+Alt+Up", "control+alt+ArrowUp"),
        ("Ctrl+Plus", "control+Equal"),
        ("Ctrl+,", "control+Comma"),
    ];
    for (accel, want) in table {
        assert_eq!(accel_to_shortcut(accel).as_deref(), Some(want), "{accel}");
    }
}

#[test]
fn needs_ctrl_alt_or_super_and_exactly_one_key() {
    for bad in ["", "   ", "Shift+A", "A", "Space", "F12", "Ctrl", "Ctrl+Alt", "Ctrl+A+B", "Ctrl+", "+A", "Ctrl++A"] {
        assert_eq!(accel_to_shortcut(bad), None, "{bad:?}");
    }
}

#[test]
fn unknown_keys_and_modifiers_are_none() {
    for bad in ["Ctrl+Banana", "Hyper+A", "Ctrl+F25", "Ctrl+num10", "Ctrl+!", "AltGr+A", "Ctrl+AB"] {
        assert_eq!(accel_to_shortcut(bad), None, "{bad:?}");
    }
}

#[test]
fn every_key_the_settings_drawer_can_produce() {
    let cases = [
        // Letters, digits, F keys, numpad digits.
        ("Ctrl+B", "control+KeyB"),
        ("Alt+z", "alt+KeyZ"),
        ("Ctrl+5", "control+Digit5"),
        ("Alt+F1", "alt+F1"),
        ("Alt+F24", "alt+F24"),
        ("Ctrl+num0", "control+Numpad0"),
        // CODE_KEYS in SettingsDrawer.tsx.
        ("Ctrl+Enter", "control+Enter"),
        ("Ctrl+Tab", "control+Tab"),
        ("Ctrl+Backspace", "control+Backspace"),
        ("Ctrl+Delete", "control+Delete"),
        ("Ctrl+Insert", "control+Insert"),
        ("Ctrl+Home", "control+Home"),
        ("Ctrl+End", "control+End"),
        ("Ctrl+PageUp", "control+PageUp"),
        ("Ctrl+PageDown", "control+PageDown"),
        ("Alt+Down", "alt+ArrowDown"),
        ("Alt+Left", "alt+ArrowLeft"),
        ("Alt+Right", "alt+ArrowRight"),
        ("Ctrl+-", "control+Minus"),
        ("Ctrl+=", "control+Equal"),
        ("Ctrl+[", "control+BracketLeft"),
        ("Ctrl+]", "control+BracketRight"),
        ("Ctrl+\\", "control+Backslash"),
        ("Ctrl+;", "control+Semicolon"),
        ("Ctrl+'", "control+Quote"),
        ("Ctrl+.", "control+Period"),
        ("Ctrl+/", "control+Slash"),
        ("Ctrl+`", "control+Backquote"),
        ("Ctrl+numadd", "control+NumpadAdd"),
        ("Ctrl+numsub", "control+NumpadSubtract"),
        ("Ctrl+nummult", "control+NumpadMultiply"),
        ("Ctrl+numdiv", "control+NumpadDivide"),
        ("Ctrl+numdec", "control+NumpadDecimal"),
        // All four modifiers, in SettingsDrawer order, come out in canonical order.
        ("Ctrl+Alt+Shift+Super+K", "shift+control+alt+super+KeyK"),
    ];
    for (accel, want) in cases {
        assert_eq!(accel_to_shortcut(accel).as_deref(), Some(want), "{accel}");
    }
}

#[test]
fn other_electron_spellings() {
    let cases = [
        ("Control+Space", "control+Space"),
        ("CommandOrControl+Space", "control+Space"),
        ("CmdOrCtrl+Space", "control+Space"),
        ("Option+Space", "alt+Space"),
        ("Meta+Space", "super+Space"),
        ("Cmd+Space", "super+Space"),
        ("Command+Space", "super+Space"),
        ("ALT+SPACE", "alt+Space"),
        (" Alt + Space ", "alt+Space"),
        ("Ctrl+Return", "control+Enter"),
        ("Ctrl+Esc", "control+Escape"),
        ("Ctrl+Escape", "control+Escape"),
        ("Ctrl+Capslock", "control+CapsLock"),
        ("Ctrl+Numlock", "control+NumLock"),
        ("Ctrl+Scrolllock", "control+ScrollLock"),
        ("Ctrl+PrintScreen", "control+PrintScreen"),
        ("Ctrl+VolumeUp", "control+AudioVolumeUp"),
        ("Ctrl+VolumeDown", "control+AudioVolumeDown"),
        ("Ctrl+VolumeMute", "control+AudioVolumeMute"),
        ("Ctrl+MediaNextTrack", "control+MediaTrackNext"),
        ("Ctrl+MediaPreviousTrack", "control+MediaTrackPrevious"),
        ("Ctrl+MediaStop", "control+MediaStop"),
        ("Ctrl+MediaPlayPause", "control+MediaPlayPause"),
        ("Ctrl+Ctrl+A", "control+KeyA"),
    ];
    for (accel, want) in cases {
        assert_eq!(accel_to_shortcut(accel).as_deref(), Some(want), "{accel}");
    }
}
