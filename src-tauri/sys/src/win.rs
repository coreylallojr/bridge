//! Small shared Win32 helpers (crate-private).
use sha1::{Digest, Sha1};
use windows::core::PWSTR;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
use windows::Win32::System::Diagnostics::Debug::{
    FormatMessageW, FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS,
};

/// NUL-terminated UTF-16 for Win32 `PCWSTR` parameters.
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Lower-case hex SHA-1 of a string (icon file names, same as the TS `createHash('sha1')`).
pub(crate) fn sha1_hex(s: &str) -> String {
    format!("{:x}", Sha1::digest(s.as_bytes()))
}

/// Win32 error code from a `windows::core::Error` (unwraps `HRESULT_FROM_WIN32`).
pub(crate) fn win32_code(e: &windows::core::Error) -> u32 {
    let hr = e.code().0 as u32;
    if hr & 0xFFFF_0000 == 0x8007_0000 {
        hr & 0xFFFF
    } else {
        hr
    }
}

/// Windows' own text for an error code via `FormatMessageW`, with the trailing dot/whitespace stripped.
pub(crate) fn system_message(code: u32) -> String {
    let mut buf = [0u16; 1024];
    // SAFETY: the buffer outlives the call and its length is passed.
    let n = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
            None,
            code,
            0,
            PWSTR(buf.as_mut_ptr()),
            buf.len() as u32,
            None,
        )
    };
    let text = String::from_utf16_lossy(&buf[..(n as usize).min(buf.len())]);
    let text = strip_trailing(&text);
    if text.is_empty() {
        format!("error {code}")
    } else {
        text.to_string()
    }
}

/// `s.replace(/[.\s]+$/, '')`
pub(crate) fn strip_trailing(s: &str) -> &str {
    s.trim_end_matches(|c: char| c == '.' || c.is_whitespace())
}

/// Runs `f` on a fresh thread with an STA COM apartment (Shell APIs want one), and waits for it.
/// Safe to call from any thread, including async runtime workers.
pub(crate) fn on_com_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    std::thread::spawn(move || {
        // SAFETY: balanced with CoUninitialize below on this same thread.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        let out = f();
        if hr.is_ok() {
            unsafe { CoUninitialize() };
        }
        out
    })
    .join()
    .ok()
}
