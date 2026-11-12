//! Start-menu app list via `Get-StartApps` (the run/cache half of `src/main/launch/apps.ts`).
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bridge_core::apps::parse_start_apps;
use bridge_core::types::AppEntry;

const PS_COMMAND: &str = "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
                          Get-StartApps | Select-Object Name,AppID | ConvertTo-Json -Compress";
const TIMEOUT: Duration = Duration::from_secs(20);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `Some` once a run succeeded. Held locked during a run, so concurrent callers share it.
static CACHE: Mutex<Option<Vec<AppEntry>>> = Mutex::new(None);

fn run_get_start_apps() -> Result<Vec<AppEntry>, String> {
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", PS_COMMAND])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("timed out".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let out = reader.join().map_err(|_| "reader panicked")?;
    if !status.success() {
        return Err(format!("exit {status}"));
    }
    // [Console]::OutputEncoding = UTF8 can emit a BOM (JS `trim()` dropped it; Rust's doesn't).
    let text = String::from_utf8_lossy(&out);
    Ok(parse_start_apps(text.trim_start_matches('\u{feff}')))
}

/// Start-menu apps, sorted by name. Runs PowerShell once and caches the answer; a failure gives `[]` and is
/// retried on the next call. Blocks (up to 20 s on the first call): call it off the UI thread.
pub fn list_start_apps() -> Vec<AppEntry> {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(list) = cache.as_ref() {
        return list.clone();
    }
    match run_get_start_apps() {
        Ok(list) => {
            *cache = Some(list.clone());
            list
        }
        Err(err) => {
            eprintln!("[apps] Get-StartApps failed: {err}");
            Vec::new()
        }
    }
}

/// Warms the cache on a background thread (call at startup so the app picker opens instantly).
pub fn prefetch_start_apps() {
    std::thread::spawn(|| {
        let _ = list_start_apps();
    });
}
