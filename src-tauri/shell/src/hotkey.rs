//! The global "show Bridge" hotkey (default Alt+Space). Port of `src/main/shell/hotkey.ts`.
//! The profile stores Electron accelerator syntax; core `accel_to_shortcut` converts it for `global-hotkey`.
use bridge_core::accel::accel_to_shortcut;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Runtime, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

static REGISTERED: Mutex<Option<Shortcut>> = Mutex::new(None);

/// The plugin, ready for `tauri::Builder::plugin(bridge_shell::hotkey::plugin())`.
pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new().build()
}

/// Electron accelerator → parsed shortcut, or None if Bridge can't use it.
pub(crate) fn parse(accel: &str) -> Option<Shortcut> {
    if accel.trim().is_empty() {
        return None;
    }
    Shortcut::from_str(&accel_to_shortcut(accel)?).ok()
}

/// Unregisters the previous Bridge hotkey, then registers `accel`.
/// Returns false if another app already owns it (e.g. PowerToys on Alt+Space) or the accelerator is invalid.
pub fn register<R: Runtime>(app: &AppHandle<R>, accel: &str, cb: Arc<dyn Fn() + Send + Sync>) -> bool {
    unregister(app);
    let Some(shortcut) = parse(accel) else { return false };
    let ok = app
        .global_shortcut()
        .on_shortcut(shortcut, move |_app, _s, ev| {
            if ev.state == ShortcutState::Pressed {
                cb();
            }
        })
        .is_ok();
    if ok {
        *REGISTERED.lock().unwrap_or_else(|e| e.into_inner()) = Some(shortcut);
    }
    ok
}

/// Drops Bridge's hotkey, if any.
pub fn unregister<R: Runtime>(app: &AppHandle<R>) {
    let prev = REGISTERED.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(s) = prev {
        let _ = app.global_shortcut().unregister(s); // already gone is fine
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn the_profile_hotkeys_parse() {
        for a in ["Alt+Space", "Ctrl+Shift+F12", "Super+num5", "Ctrl+Alt+Up", "Ctrl+Plus", "Ctrl+,"] {
            assert!(parse(a).is_some(), "{a}");
        }
    }

    #[test]
    fn unusable_hotkeys_are_refused() {
        for a in ["", "   ", "Shift+A", "Nonsense+Q"] {
            assert!(parse(a).is_none(), "{a}");
        }
    }
}
