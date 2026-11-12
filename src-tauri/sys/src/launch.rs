//! Executes a `LaunchPlan` (port of `src/main/launch/launch.ts`). Pure planning lives in `bridge_core::launch_plan`.
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use bridge_core::launch_plan::find_edge;
pub use bridge_core::launch_plan::LaunchPlan;
use bridge_core::types::{Res, Tile, TileKind};
use serde::Serialize;
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::win::{on_com_thread, strip_trailing, system_message, wide, win32_code};

/// `None` = not looked up yet; `Some(None)` = Edge isn't installed.
static EDGE: Mutex<Option<Option<PathBuf>>> = Mutex::new(None);

/// Microsoft Edge's exe (looked up once, cached; forgotten again if spawning it fails).
pub fn edge_path() -> Option<PathBuf> {
    let mut cached = EDGE.lock().unwrap_or_else(|e| e.into_inner());
    cached
        .get_or_insert_with(|| find_edge(&|name: &str| std::env::var(name).ok(), &|p: &Path| p.exists()))
        .clone()
}

fn forget_edge() {
    *EDGE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn opened(tile: &Tile) -> Res<()> {
    Res::ok(format!("Opened {}", tile.name))
}

fn fail(tile: &Tile, reason: &str) -> Res<()> {
    Res::fail(format!("Couldn't open {}: {}.", tile.name, reason))
}

fn missing_reason(tile: &Tile, path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".exe") {
        "the program isn't there anymore"
    } else if lower.ends_with(".lnk") {
        "the shortcut isn't there anymore"
    } else if tile.kind == TileKind::Folder {
        "the folder doesn't exist anymore"
    } else {
        "the file doesn't exist anymore"
    }
}

/// One dry-launch log line. `plan` uses core's serde, which is the TS shape (`{how:'spawn', file, args, cwd?}` …).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DryLine<'a> {
    at: String,
    tile_id: &'a str,
    name: &'a str,
    plan: &'a LaunchPlan,
}

/// `new Date().toISOString()`
fn iso_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn log_dry_launch(home: &Path, tile: &Tile, plan: &LaunchPlan) -> std::io::Result<()> {
    let line = DryLine { at: iso_now(), tile_id: &tile.id, name: &tile.name, plan };
    let json = serde_json::to_string(&line).map_err(std::io::Error::other)?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(home.join("launch-log.jsonl"))?;
    f.write_all(format!("{json}\n").as_bytes())
}

/// Starts a program detached. `Err` is a reason in the user's words.
fn spawn_detached(file: &str, args: &[String], cwd: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new(file);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // CREATE_NEW_PROCESS_GROUP: Bridge closing never takes the program with it (libuv `detached`).
    cmd.creation_flags(0x0000_0200);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    match cmd.spawn() {
        Ok(_child) => Ok(()), // dropping a Child neither waits for nor kills it
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err("Windows couldn't find the program".into()),
        Err(e) => {
            let detail = match e.raw_os_error() {
                Some(code) => system_message(code as u32),
                None => e.to_string(),
            };
            Err(format!("Windows wouldn't start it ({detail})"))
        }
    }
}

/// `ShellExecuteExW(verb "open")` on an STA thread. `Err` is Windows' own message (trailing dot stripped).
pub(crate) fn shell_open(target: &str) -> Result<(), String> {
    let target = target.to_string();
    on_com_thread(move || {
        let file = wide(&target);
        let verb = wide("open");
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC,
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(file.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        // SAFETY: `info` and the wide strings it points to outlive the call.
        match unsafe { ShellExecuteExW(&mut info) } {
            Ok(()) => Ok(()),
            Err(e) => Err(strip_trailing(&system_message(win32_code(&e))).to_string()),
        }
    })
    .unwrap_or_else(|| Err("something went wrong".into()))
}

/// Opens a tile following `plan`. Never panics; the message is safe to show the user as-is.
/// `dry` = append `{at,tileId,name,plan}` to `<home>\launch-log.jsonl` instead of opening anything.
pub fn execute(plan: LaunchPlan, tile: &Tile, home: &Path, dry: bool) -> Res<()> {
    if let LaunchPlan::Refuse { reason } = &plan {
        return fail(tile, reason);
    }
    if dry {
        return match log_dry_launch(home, tile, &plan) {
            Ok(()) => opened(tile),
            Err(e) => fail(tile, &format!("something went wrong ({e})")),
        };
    }
    match plan {
        LaunchPlan::Spawn { file, args, cwd } => {
            if cwd.is_some() && !Path::new(&file).exists() {
                return fail(tile, missing_reason(tile, &file));
            }
            let cwd = cwd.filter(|d| Path::new(d).is_dir());
            match spawn_detached(&file, &args, cwd.as_deref()) {
                Ok(()) => opened(tile),
                Err(err) => {
                    let is_edge = edge_path().is_some_and(|e| e.as_os_str().eq_ignore_ascii_case(&file));
                    if is_edge {
                        // Edge vanished (uninstalled/moved): forget it and let the default browser handle the link.
                        forget_edge();
                        if let Some(url) = args.last() {
                            let _ = shell_open(url);
                        }
                        return opened(tile);
                    }
                    fail(tile, &err)
                }
            }
        }
        LaunchPlan::OpenPath { path } => {
            if !Path::new(&path).exists() {
                return fail(tile, missing_reason(tile, &path));
            }
            match shell_open(&path) {
                Ok(()) => opened(tile),
                Err(err) => fail(tile, &err),
            }
        }
        LaunchPlan::OpenExternal { url } => match shell_open(&url) {
            Ok(()) => opened(tile),
            Err(_) => fail(tile, "Windows has no app for that kind of link"),
        },
        LaunchPlan::Refuse { reason } => fail(tile, &reason),
    }
}
