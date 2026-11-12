//! Window messages Tauri doesn't surface the way Bridge needs them, caught by subclassing the main HWND:
//! - `WM_DISPLAYCHANGE` and `WM_SETTINGCHANGE(SPI_SETWORKAREA)`: screens added/removed/resized, taskbar moved;
//! - `WM_ACTIVATE(WA_INACTIVE)`: the window lost activation (Electron's `blur`). Focus moving into the WebView2
//!   child doesn't deactivate the top-level window, so this never fires for clicks inside Bridge.
//!
//! Callbacks run on the window's thread inside its window procedure: they must only schedule work, never block.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_SETWORKAREA, WA_INACTIVE, WM_ACTIVATE, WM_DISPLAYCHANGE, WM_NCDESTROY, WM_SETTINGCHANGE,
};

const SUBCLASS_ID: usize = 0x4252_4447; // "BRDG"

pub struct Callbacks {
    pub on_displays: Box<dyn Fn() + Send + Sync>,
    pub on_deactivate: Box<dyn Fn() + Send + Sync>,
}

/// What a window message means for Bridge (pure, so it's unit-tested).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Signal {
    Displays,
    Deactivated,
    None,
}

pub(crate) fn classify(msg: u32, wparam: usize) -> Signal {
    match msg {
        WM_DISPLAYCHANGE => Signal::Displays,
        WM_SETTINGCHANGE if wparam == SPI_SETWORKAREA.0 as usize => Signal::Displays,
        WM_ACTIVATE if (wparam & 0xffff) as u32 == WA_INACTIVE => Signal::Deactivated,
        _ => Signal::None,
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
            drop(Box::from_raw(data as *mut Callbacks));
            return DefSubclassProc(hwnd, msg, wparam, lparam);
        }
    }
    let cbs = unsafe { &*(data as *const Callbacks) };
    // A panic must never unwind across the window procedure.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match classify(msg, wparam.0) {
        Signal::Displays => (cbs.on_displays)(),
        Signal::Deactivated => (cbs.on_deactivate)(),
        Signal::None => {}
    }));
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Subclasses `hwnd`. Must run on the thread that owns the window (Tauri's main thread).
pub fn install(hwnd: HWND, cbs: Callbacks) -> bool {
    let data = Box::into_raw(Box::new(cbs)) as usize;
    let ok = unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, data) }.as_bool();
    if !ok {
        drop(unsafe { Box::from_raw(data as *mut Callbacks) });
    }
    ok
}

/// Trailing-edge debounce: only the last call in a burst runs, `delay` after it.
#[derive(Clone)]
pub struct Debounce {
    gen: Arc<AtomicU64>,
    delay: Duration,
}

impl Debounce {
    pub fn new(delay: Duration) -> Self {
        Debounce { gen: Arc::new(AtomicU64::new(0)), delay }
    }

    pub fn call(&self, f: impl FnOnce() + Send + 'static) {
        let mine = self.gen.fetch_add(1, Ordering::SeqCst) + 1;
        let (gen, delay) = (self.gen.clone(), self.delay);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            if gen.load(Ordering::SeqCst) == mine {
                f();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn classifies_the_messages_bridge_cares_about() {
        assert_eq!(classify(WM_DISPLAYCHANGE, 32), Signal::Displays);
        assert_eq!(classify(WM_SETTINGCHANGE, SPI_SETWORKAREA.0 as usize), Signal::Displays);
        assert_eq!(classify(WM_SETTINGCHANGE, 0), Signal::None);
        assert_eq!(classify(WM_ACTIVATE, 0), Signal::Deactivated);
        // WA_INACTIVE with the minimized flag in the high word is still a deactivation.
        assert_eq!(classify(WM_ACTIVATE, 0x0001_0000), Signal::Deactivated);
        assert_eq!(classify(WM_ACTIVATE, 1), Signal::None); // WA_ACTIVE
        assert_eq!(classify(WM_ACTIVATE, 2), Signal::None); // WA_CLICKACTIVE
        assert_eq!(classify(0x0400, 0), Signal::None);
    }

    #[test]
    fn a_burst_runs_once_after_it_settles() {
        let d = Debounce::new(Duration::from_millis(100));
        let n = Arc::new(AtomicUsize::new(0));
        for _ in 0..5 {
            let n = n.clone();
            d.call(move || {
                n.fetch_add(1, Ordering::SeqCst);
            });
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(n.load(Ordering::SeqCst), 0, "nothing runs mid-burst");
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(n.load(Ordering::SeqCst), 1);
    }
}
