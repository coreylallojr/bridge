//! Electron accelerator strings (what Settings saves, see `SettingsDrawer.tsx` `toAccelerator`) to the string format
//! `global-hotkey` 0.8 parses (`HotKey::from_str`, used by tauri-plugin-global-shortcut).
//!
//! Output is global-hotkey's canonical form (`HotKey::into_string`): `shift+control+alt+super+<Code>`, e.g.
//! `Alt+Space` -> `alt+Space`, `Ctrl+Shift+F12` -> `shift+control+F12`, `Super+num5` -> `super+Numpad5`.

/// `None` when the accelerator is empty, malformed, uses an unknown key, or has no Ctrl/Alt/Super
/// (Shift alone would steal normal typing).
pub fn accel_to_shortcut(accel: &str) -> Option<String> {
    let tokens: Vec<&str> = accel.split('+').map(str::trim).collect();
    let (key, mods) = tokens.split_last()?;
    let (mut shift, mut control, mut alt, mut sup) = (false, false, false, false);
    for m in mods {
        match m.to_ascii_lowercase().as_str() {
            "shift" => shift = true,
            "ctrl" | "control" | "commandorcontrol" | "cmdorctrl" | "commandorctrl" | "cmdorcontrol" => control = true,
            "alt" | "option" => alt = true,
            "super" | "meta" | "cmd" | "command" => sup = true,
            _ => return None,
        }
    }
    if !(control || alt || sup) {
        return None;
    }
    let code = key_code(key)?;
    let mut out = String::new();
    for (on, name) in [(shift, "shift+"), (control, "control+"), (alt, "alt+"), (sup, "super+")] {
        if on {
            out.push_str(name);
        }
    }
    out.push_str(&code);
    Some(out)
}

/// Electron key name (case-insensitive) -> W3C `Code` name as printed by keyboard-types.
fn key_code(key: &str) -> Option<String> {
    let k = key.to_ascii_lowercase();
    let b = k.as_bytes();
    if b.len() == 1 && b[0].is_ascii_lowercase() {
        return Some(format!("Key{}", k.to_ascii_uppercase()));
    }
    if b.len() == 1 && b[0].is_ascii_digit() {
        return Some(format!("Digit{k}"));
    }
    if let Some(n) = k.strip_prefix('f').filter(|n| !n.is_empty() && !n.starts_with('0') && n.bytes().all(|c| c.is_ascii_digit())) {
        return n.parse::<u8>().ok().filter(|n| (1..=24).contains(n)).map(|n| format!("F{n}"));
    }
    if let Some(d) = k.strip_prefix("num").filter(|d| d.len() == 1 && d.as_bytes()[0].is_ascii_digit()) {
        return Some(format!("Numpad{d}"));
    }
    let code = match k.as_str() {
        "space" => "Space",
        "tab" => "Tab",
        "enter" | "return" => "Enter",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "insert" => "Insert",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "escape" | "esc" => "Escape",
        "capslock" => "CapsLock",
        "numlock" => "NumLock",
        "scrolllock" => "ScrollLock",
        "printscreen" => "PrintScreen",
        // Electron's `Plus` is the =/+ key (VK_OEM_PLUS).
        "plus" | "=" => "Equal",
        "-" => "Minus",
        "[" => "BracketLeft",
        "]" => "BracketRight",
        "\\" => "Backslash",
        ";" => "Semicolon",
        "'" => "Quote",
        "," => "Comma",
        "." => "Period",
        "/" => "Slash",
        "`" => "Backquote",
        "numadd" => "NumpadAdd",
        "numsub" => "NumpadSubtract",
        "nummult" => "NumpadMultiply",
        "numdiv" => "NumpadDivide",
        "numdec" => "NumpadDecimal",
        "volumeup" => "AudioVolumeUp",
        "volumedown" => "AudioVolumeDown",
        "volumemute" => "AudioVolumeMute",
        "medianexttrack" => "MediaTrackNext",
        "mediaprevioustrack" => "MediaTrackPrevious",
        "mediastop" => "MediaStop",
        "mediaplaypause" => "MediaPlayPause",
        _ => return None,
    };
    Some(code.to_string())
}
