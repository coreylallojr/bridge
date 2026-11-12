//! The Bridge window and its display modes. Port of `src/main/shell/window.ts`; behavior is identical:
//! - Pinned: fills a chosen screen's work area like a live wallpaper; on blur it drops to the bottom of the z-order.
//!   Never always-on-top.
//! - Summon: pops up on the screen under the cursor; blur or Esc hides it.
//! - `forced` (BRIDGE_WINDOW=WxH): a plain fixed-size window for screenshots/E2E (no mode logic, no blur behavior).
//!
//! Threading: every window change runs on Tauri's main (UI) thread. Called from elsewhere, it is dispatched there
//! and waited for, so callers see the same synchronous behavior as Electron. Nothing here ever holds a lock while
//! touching the window.
use crate::display_events::{self, Callbacks, Debounce};
use crate::displays::{self, Mon};
use crate::zorder;
use bridge_core::displays::{pick_display, screen_number_of, to_display_info, RawDisplay};
use bridge_core::{DisplayInfo, Settings, ShellMode, ShellState};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::window::Color;
use tauri::{AppHandle, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

pub const BROWSER_ARGS: &str =
    "--disable-background-networking --disable-sync --disable-extensions --disable-features=Translate,msSmartScreenProtection";
const DEFAULT_SIZE: (u32, u32) = (1600, 900);
const DISPLAY_SETTLE: Duration = Duration::from_millis(300);

type ShellCallback = Arc<dyn Fn(ShellState) + Send + Sync>;

struct St {
    settings: Settings,
    mode: ShellMode,
    display_id: i64,
    hotkey: String,
    hotkey_ok: bool,
}

#[derive(Default)]
struct Loaded {
    done: bool,
    waiting: Vec<Box<dyn FnOnce() + Send>>,
}

pub struct WindowManager {
    app: AppHandle,
    win: WebviewWindow,
    /// Raw HWND (a pointer isn't Send).
    hwnd: isize,
    forced: Option<(u32, u32)>,
    profile_line: String,
    st: Mutex<St>,
    quitting: AtomicBool,
    on_change: Mutex<Option<ShellCallback>>,
    loaded: Mutex<Loaded>,
    debounce: Debounce,
}

/// Only Bridge's own pages may load in the window (Electron's `will-navigate` guard).
pub(crate) fn navigation_allowed(url: &Url, dev: Option<&Url>) -> bool {
    match url.scheme() {
        "tauri" => true,
        "http" | "https" => {
            url.host_str() == Some("tauri.localhost") || dev.is_some_and(|d| d.origin() == url.origin())
        }
        "about" => url.as_str() == "about:blank",
        _ => false,
    }
}

impl WindowManager {
    /// Creates the hidden `main` window and wires its behavior. It shows itself when the page has loaded.
    /// `home` is `BRIDGE_HOME` (the WebView2 data folder lives in `<home>\webview`).
    pub fn new(
        app: &AppHandle,
        home: &Path,
        forced: Option<(u32, u32)>,
        profile_line: String,
    ) -> tauri::Result<Arc<WindowManager>> {
        let slot: Arc<OnceLock<Weak<WindowManager>>> = Arc::new(OnceLock::new());
        let loaded_early = Arc::new(AtomicBool::new(false));
        let (w, h) = forced.unwrap_or(DEFAULT_SIZE);
        let dev = if tauri::is_dev() { app.config().build.dev_url.clone() } else { None };

        let (slot_load, early) = (slot.clone(), loaded_early.clone());
        let win = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title("Bridge")
            .inner_size(w as f64, h as f64)
            .center()
            .decorations(false)
            .resizable(false)
            .maximizable(false)
            .visible(false)
            .skip_taskbar(true)
            .background_color(Color(0, 0, 0, 255))
            .disable_drag_drop_handler()
            .data_directory(home.join("webview"))
            .additional_browser_args(BROWSER_ARGS)
            .on_navigation(move |url| navigation_allowed(url, dev.as_ref()))
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .on_page_load(move |_w, p| {
                if p.event() != PageLoadEvent::Finished {
                    return;
                }
                match slot_load.get().and_then(Weak::upgrade) {
                    Some(wm) => wm.page_loaded(),
                    None => early.store(true, Ordering::SeqCst),
                }
            })
            .build()?;
        let hwnd = win.hwnd()?.0 as isize;

        let (_, primary) = displays::monitors(app);
        let defaults = Settings::default();
        let wm = Arc::new(WindowManager {
            app: app.clone(),
            win,
            hwnd,
            forced,
            profile_line,
            st: Mutex::new(St {
                hotkey: defaults.hotkey.clone(),
                settings: defaults,
                mode: ShellMode::Summon,
                display_id: primary,
                hotkey_ok: true,
            }),
            quitting: AtomicBool::new(false),
            on_change: Mutex::new(None),
            loaded: Mutex::new(Loaded::default()),
            debounce: Debounce::new(DISPLAY_SETTLE),
        });
        let _ = slot.set(Arc::downgrade(&wm));

        if forced.is_none() {
            let weak = Arc::downgrade(&wm);
            wm.win.on_window_event(move |ev| {
                if let WindowEvent::CloseRequested { api, .. } = ev {
                    if let Some(wm) = weak.upgrade() {
                        if !wm.quitting.load(Ordering::SeqCst) {
                            api.prevent_close();
                            wm.hide();
                        }
                    }
                }
            });
            let (on_displays, on_deactivate) = (Arc::downgrade(&wm), Arc::downgrade(&wm));
            let cbs = Callbacks {
                on_displays: Box::new(move || {
                    if let Some(wm) = on_displays.upgrade() {
                        wm.displays_changed();
                    }
                }),
                on_deactivate: Box::new(move || {
                    // Deferred: never re-enter the window procedure that is delivering WM_ACTIVATE.
                    let weak = on_deactivate.clone();
                    if let Some(wm) = weak.upgrade() {
                        let _ = wm.app.run_on_main_thread(move || {
                            if let Some(wm) = weak.upgrade() {
                                wm.on_blur();
                            }
                        });
                    }
                }),
            };
            let raw = hwnd;
            // SetWindowSubclass only works on the window's own thread.
            wm.win.run_on_main_thread(move || {
                if !display_events::install(HWND(raw as *mut _), cbs) {
                    eprintln!("[bridge] could not watch display changes");
                }
            })?;
        }

        if loaded_early.load(Ordering::SeqCst) {
            wm.page_loaded();
        }
        Ok(wm)
    }

    /// The `main` webview window (for events, drag-drop install, etc.).
    pub fn window(&self) -> &WebviewWindow {
        &self.win
    }

    /// Runs `f` once the page has finished its first load (right away if it already has).
    pub fn on_ready(&self, f: Box<dyn FnOnce() + Send>) {
        let mut l = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        if l.done {
            drop(l);
            f();
        } else {
            l.waiting.push(f);
        }
    }

    /// Called when displays changed and the window was re-placed.
    pub fn on_shell_change(&self, cb: Box<dyn Fn(ShellState) + Send + Sync>) {
        *self.on_change.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::from(cb));
    }

    /// Picks mode + screen from settings and the connected displays, and places the window.
    pub fn apply(self: &Arc<Self>, settings: &Settings) -> ShellState {
        let s = settings.clone();
        self.on_ui(move |wm| wm.apply_ui(&s)).unwrap_or_else(|| self.state())
    }

    pub fn set_hotkey_state(&self, hotkey: &str, ok: bool) {
        let mut st = self.lock();
        st.hotkey = hotkey.to_string();
        st.hotkey_ok = ok;
    }

    pub fn state(&self) -> ShellState {
        let (raws, primary) = displays::list(&self.app);
        self.state_with(&to_display_info(&raws, primary))
    }

    pub fn show(self: &Arc<Self>) {
        let _ = self.on_ui(|wm| wm.show_ui());
    }

    pub fn hide(self: &Arc<Self>) {
        let _ = self.on_ui(|wm| wm.hide_ui());
    }

    pub fn toggle(self: &Arc<Self>) {
        let _ = self.on_ui(|wm| {
            if zorder::is_visible(wm.hwnd()) && zorder::is_focused(wm.hwnd()) {
                wm.hide_ui()
            } else {
                wm.show_ui()
            }
        });
    }

    /// Lets the window really close (quit). Call before `app.exit`.
    pub fn allow_close(&self) {
        self.quitting.store(true, Ordering::SeqCst);
    }

    // ---- internals ----------------------------------------------------------------------------------------------

    fn lock(&self) -> std::sync::MutexGuard<'_, St> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }

    fn mode(&self) -> ShellMode {
        self.lock().mode
    }

    fn is_ui_thread(&self) -> bool {
        unsafe { GetWindowThreadProcessId(self.hwnd(), None) == GetCurrentThreadId() }
    }

    /// Runs `f` on the UI thread and waits for it. None if the event loop is gone (quitting).
    fn on_ui<T: Send + 'static>(self: &Arc<Self>, f: impl FnOnce(&Arc<WindowManager>) -> T + Send + 'static) -> Option<T> {
        if self.is_ui_thread() {
            return Some(f(self));
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let me = self.clone();
        self.app.run_on_main_thread(move || {
            let _ = tx.send(f(&me));
        })
        .ok()?;
        rx.recv().ok()
    }

    fn page_loaded(self: &Arc<Self>) {
        let waiting = {
            let mut l = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
            if l.done {
                return; // a reload: the window was already shown once
            }
            l.done = true;
            std::mem::take(&mut l.waiting)
        };
        if self.forced.is_some() {
            let _ = self.win.show();
        } else {
            self.show();
        }
        for f in waiting {
            f();
        }
    }

    fn state_with(&self, infos: &[DisplayInfo]) -> ShellState {
        let st = self.lock();
        ShellState {
            mode: st.mode,
            display_id: st.display_id,
            screen_number: screen_number_of(infos, st.display_id),
            hotkey: st.hotkey.clone(),
            hotkey_ok: st.hotkey_ok,
            profile_line: self.profile_line.clone(),
        }
    }

    fn apply_ui(&self, settings: &Settings) -> ShellState {
        let (mons, primary) = displays::monitors(&self.app);
        let raws: Vec<RawDisplay> = mons.iter().map(|m| m.raw.clone()).collect();
        let infos = to_display_info(&raws, primary);
        let (mode, display_id) = pick_display(&infos, settings.mode, settings.pinned_display_id);
        let was = {
            let mut st = self.lock();
            st.settings = settings.clone();
            let was = st.mode;
            st.mode = mode;
            was
        };
        if self.forced.is_some() {
            self.lock().display_id = display_id;
            return self.state_with(&infos);
        }
        let _ = self.win.set_resizable(mode != ShellMode::Pinned);
        let hwnd = self.hwnd();
        if mode == ShellMode::Pinned {
            self.place_on(display_id, &mons, primary);
            // Switching into pinned: it's a wallpaper layer, so make it visible without stealing focus.
            if was != ShellMode::Pinned && !zorder::is_visible(hwnd) {
                zorder::show_inactive(hwnd);
                zorder::send_to_bottom(hwnd);
            }
        } else if zorder::is_visible(hwnd) {
            self.place_on(cursor_display(&mons, primary), &mons, primary);
        } else {
            self.lock().display_id = cursor_display(&mons, primary);
        }
        self.state_with(&infos)
    }

    fn show_ui(&self) {
        let hwnd = self.hwnd();
        if self.forced.is_none() && self.mode() == ShellMode::Summon {
            let (mons, primary) = displays::monitors(&self.app);
            self.place_on(cursor_display(&mons, primary), &mons, primary);
        }
        if zorder::is_minimized(hwnd) {
            let _ = self.win.unminimize();
        }
        let _ = self.win.show();
        // move_top raises it even if Windows refuses to hand over focus (e.g. called from a background pipe request).
        zorder::move_top(hwnd);
        let _ = self.win.set_focus();
    }

    fn hide_ui(&self) {
        let hwnd = self.hwnd();
        if self.forced.is_none() && self.mode() == ShellMode::Pinned {
            zorder::send_to_bottom(hwnd);
            if zorder::is_focused(hwnd) {
                zorder::deactivate(hwnd);
            }
        } else {
            let _ = self.win.hide();
        }
    }

    fn on_blur(&self) {
        if self.mode() == ShellMode::Pinned {
            zorder::send_to_bottom(self.hwnd());
        } else {
            let _ = self.win.hide();
        }
    }

    fn place_on(&self, display_id: i64, mons: &[Mon], primary: i64) {
        let Some(m) = pick_mon(mons, display_id, primary) else { return };
        self.lock().display_id = m.raw.id;
        let (x, y, w, h) = m.work;
        let hwnd = self.hwnd();
        if zorder::bounds(hwnd) == Some(m.work) {
            return;
        }
        // Twice on purpose: moving between screens with different scaling can size the first call with the old DPI.
        zorder::set_bounds(hwnd, x, y, w, h);
        zorder::set_bounds(hwnd, x, y, w, h);
    }

    /// Displays fire several events in a burst; re-place once things settle.
    fn displays_changed(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        self.debounce.call(move || {
            let Some(wm) = weak.upgrade() else { return };
            let settings = wm.lock().settings.clone();
            let s = wm.apply(&settings);
            let cb = wm.on_change.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(cb) = cb {
                cb(s);
            }
        });
    }
}

/// The monitor to place on: the requested one, else the primary, else any.
pub(crate) fn pick_mon(mons: &[Mon], display_id: i64, primary: i64) -> Option<&Mon> {
    mons.iter()
        .find(|m| m.raw.id == display_id)
        .or_else(|| mons.iter().find(|m| m.raw.id == primary))
        .or_else(|| mons.first())
}

/// The display under the cursor, if Bridge knows it; else the primary.
fn cursor_display(mons: &[Mon], primary: i64) -> i64 {
    displays::cursor_display_id().filter(|id| mons.iter().any(|m| m.raw.id == *id)).unwrap_or(primary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::displays::{device, to_mon};

    #[test]
    fn only_bridge_pages_may_load() {
        let dev = Url::parse("http://127.0.0.1:5173").unwrap();
        let ok = |s: &str, d: Option<&Url>| navigation_allowed(&Url::parse(s).unwrap(), d);
        assert!(ok("http://tauri.localhost/index.html", None));
        assert!(ok("tauri://localhost/index.html", None));
        assert!(ok("about:blank", None));
        assert!(ok("http://127.0.0.1:5173/index.html", Some(&dev)));
        assert!(!ok("http://127.0.0.1:5173/index.html", None));
        assert!(!ok("http://127.0.0.1:5174/", Some(&dev)));
        assert!(!ok("https://example.com/", Some(&dev)));
        assert!(!ok("https://tauri.localhost.evil.com/", None));
        assert!(!ok("file:///C:/Windows/win.ini", None));
        assert!(!ok("javascript:alert(1)", None));
    }

    #[test]
    fn places_on_the_requested_screen_else_the_primary_else_any() {
        let a = to_mon(&device(1), (0, 0), (1280, 720), 1.0, (0, 0, 1280, 680));
        let b = to_mon(&device(2), (1280, 0), (1920, 1080), 1.0, (1280, 0, 1920, 1040));
        let mons = vec![a.clone(), b.clone()];
        assert_eq!(pick_mon(&mons, b.raw.id, a.raw.id), Some(&b));
        assert_eq!(pick_mon(&mons, 12345, b.raw.id), Some(&b));
        assert_eq!(pick_mon(&mons, 12345, 999), Some(&a));
        assert_eq!(pick_mon(&[], 1, 1), None);
    }
}
