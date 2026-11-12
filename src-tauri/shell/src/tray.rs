//! System tray icon + menu. Bridge lives here when its window is hidden. Port of `src/main/shell/tray.ts`, plus
//! "Settings…".
use bridge_core::DisplayMode;
use std::sync::{Arc, OnceLock};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};

type Action = Arc<dyn Fn() + Send + Sync>;

pub struct TrayActions {
    pub show: Action,
    pub start_my_day: Action,
    pub set_mode: Arc<dyn Fn(DisplayMode) + Send + Sync>,
    pub open_settings: Action,
    pub open_data_folder: Action,
    pub quit: Action,
    /// Read when the tray is installed so the right mode is checked.
    pub current_mode: Arc<dyn Fn() -> DisplayMode + Send + Sync>,
}

pub(crate) const MODES: [(DisplayMode, &str, &str); 3] = [
    (DisplayMode::Auto, "Auto", "mode-auto"),
    (DisplayMode::Pinned, "Pinned", "mode-pinned"),
    (DisplayMode::Summon, "Summon", "mode-summon"),
];

/// What a menu item id does.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MenuAction {
    Show,
    StartMyDay,
    SetMode(DisplayMode),
    OpenSettings,
    OpenDataFolder,
    Quit,
}

pub(crate) fn action_for(id: &str) -> Option<MenuAction> {
    Some(match id {
        "show" => MenuAction::Show,
        "start" => MenuAction::StartMyDay,
        "settings" => MenuAction::OpenSettings,
        "data" => MenuAction::OpenDataFolder,
        "quit" => MenuAction::Quit,
        _ => MenuAction::SetMode(MODES.iter().find(|(_, _, mid)| *mid == id)?.0),
    })
}

/// The installed tray. Keep it alive for the app's lifetime (dropping the icon removes it).
pub struct Tray {
    icon: OnceLock<TrayIcon<Wry>>,
    modes: Vec<(DisplayMode, CheckMenuItem<Wry>)>,
}

impl Tray {
    /// Checks the Display-mode item for `mode` (radio behavior).
    pub fn refresh(&self, mode: DisplayMode) {
        for (m, item) in &self.modes {
            let _ = item.set_checked(*m == mode);
        }
    }
}

pub fn install(app: &AppHandle, actions: TrayActions) -> tauri::Result<Arc<Tray>> {
    let current = (actions.current_mode)();
    let show = MenuItem::with_id(app, "show", "Show Bridge", true, None::<&str>)?;
    let start = MenuItem::with_id(app, "start", "Start my day", true, None::<&str>)?;
    let mut modes = Vec::new();
    for (mode, label, id) in MODES {
        modes.push((mode, CheckMenuItem::with_id(app, id, label, true, mode == current, None::<&str>)?));
    }
    let mode_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = modes.iter().map(|(_, i)| i as _).collect();
    let display = Submenu::with_items(app, "Display mode", true, &mode_refs)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let data = MenuItem::with_id(app, "data", "Open data folder", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Bridge", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &start, &display, &settings, &data, &sep, &quit])?;

    let tray = Arc::new(Tray { icon: OnceLock::new(), modes });
    let weak = Arc::downgrade(&tray);
    let on_click = actions.show.clone();
    let mut b = TrayIconBuilder::with_id("bridge")
        .tooltip("Bridge")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |_app, ev| match action_for(ev.id().as_ref()) {
            Some(MenuAction::Show) => (actions.show)(),
            Some(MenuAction::StartMyDay) => (actions.start_my_day)(),
            Some(MenuAction::SetMode(m)) => {
                (actions.set_mode)(m);
                // Windows toggles a clicked check item by itself; put the radio state right.
                if let Some(t) = weak.upgrade() {
                    t.refresh(m);
                }
            }
            Some(MenuAction::OpenSettings) => (actions.open_settings)(),
            Some(MenuAction::OpenDataFolder) => (actions.open_data_folder)(),
            Some(MenuAction::Quit) => (actions.quit)(),
            None => {}
        })
        .on_tray_icon_event(move |_tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                on_click();
            }
        });
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    let _ = tray.icon.set(b.build(app)?);
    Ok(tray)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_map_to_actions() {
        assert_eq!(action_for("show"), Some(MenuAction::Show));
        assert_eq!(action_for("start"), Some(MenuAction::StartMyDay));
        assert_eq!(action_for("mode-auto"), Some(MenuAction::SetMode(DisplayMode::Auto)));
        assert_eq!(action_for("mode-pinned"), Some(MenuAction::SetMode(DisplayMode::Pinned)));
        assert_eq!(action_for("mode-summon"), Some(MenuAction::SetMode(DisplayMode::Summon)));
        assert_eq!(action_for("settings"), Some(MenuAction::OpenSettings));
        assert_eq!(action_for("data"), Some(MenuAction::OpenDataFolder));
        assert_eq!(action_for("quit"), Some(MenuAction::Quit));
        assert_eq!(action_for("nope"), None);
    }
}
