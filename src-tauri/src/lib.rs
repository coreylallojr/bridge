//! Bridge desktop app (Tauri 2). OS work lives in the `bridge-sys` crate (launching, clipboard, icons, stats,
//! metadata, drops) and `bridge-shell` (window modes, z-order, displays, tray, hotkey, single instance, CLI pipe).
//! `api` is the glue behind `window.bridge`. Pure logic lives in `bridge-core`. Port of `src/main/index.ts`.
pub mod api;

use api::{Api, ShellOps};
use bridge_core::command::parse_command;
use bridge_core::displays::to_display_info;
use bridge_core::functions::{is_default_home, profile_line, render_functions_ps1, write_functions_file};
use bridge_core::pipe_proto::pipe_short_name;
use bridge_core::profile::ProfileStore;
use bridge_core::*;
use bridge_shell::{hotkey, instance, pipe_server, tray, WindowManager};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tauri::{AppHandle, Manager};

const STARTER: &str = include_str!("../../examples/starter-profile.json");

/// `BRIDGE_HOME` or `%APPDATA%\Bridge`, created if missing.
pub fn bridge_home() -> PathBuf {
    let home = std::env::var_os("BRIDGE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("Bridge"));
    let _ = std::fs::create_dir_all(&home);
    home
}

fn log(home: &Path, line: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(home.join("bridge.log")) {
        let _ = writeln!(f, "{} {line}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }
}

fn forced_size() -> Option<(u32, u32)> {
    let s = std::env::var("BRIDGE_WINDOW").ok()?;
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// A per-home pipe, so test/demo instances never talk to the user's real Bridge.
fn pipe_short(home: &Path) -> String {
    use sha1::{Digest, Sha1};
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
    if std::env::var_os("BRIDGE_HOME").is_none() {
        return pipe_short_name(&user);
    }
    let hash = Sha1::digest(home.to_string_lossy().as_bytes());
    let hex: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
    pipe_short_name(&format!("{user}-{hex}"))
}

struct Shell {
    app: AppHandle,
    wm: Arc<WindowManager>,
    isolated: bool,
    on_hotkey: OnceLock<Arc<dyn Fn() + Send + Sync>>,
}

impl ShellOps for Shell {
    fn state(&self) -> ShellState {
        self.wm.state()
    }
    fn apply(&self, s: &Settings) -> ShellState {
        self.wm.apply(s)
    }
    fn apply_hotkey(&self, accel: &str) -> bool {
        let ok = self.isolated
            || self.on_hotkey.get().is_some_and(|cb| hotkey::register(&self.app, accel, cb.clone()));
        self.wm.set_hotkey_state(accel, ok);
        ok
    }
    fn list_displays(&self) -> Vec<DisplayInfo> {
        let (raw, primary) = bridge_shell::displays::list(&self.app);
        to_display_info(&raw, primary)
    }
    fn hide(&self) {
        self.wm.hide()
    }
    fn show(&self) {
        self.wm.show()
    }
}

pub fn run() {
    let t0 = Instant::now();
    let home = bridge_home();
    let pipe = pipe_short(&home);
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();

    // Second launch (e.g. `bridge open x` while Bridge runs): hand the command to the running one and leave.
    if !instance::acquire(&pipe) {
        let _ = instance::forward(&pipe, &argv, &cwd);
        return;
    }

    let starter = std::env::temp_dir().join("bridge-starter-profile.json");
    let _ = std::fs::write(&starter, STARTER);
    let store = match ProfileStore::open(&home, &starter) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            log(&home, &format!("could not open profile: {e}"));
            return;
        }
    };
    if let Ok(exe) = std::env::current_exe() {
        let _ = write_functions_file(&home, &render_functions_ps1(&exe.to_string_lossy(), &pipe));
    }
    let appdata = std::env::var("APPDATA").ok();
    let line = profile_line(&home, is_default_home(&home, appdata.as_deref()));
    let forced = forced_size();
    let isolated = forced.is_some();

    let tray_slot: Arc<Mutex<Option<Arc<tray::Tray>>>> = Arc::new(Mutex::new(None));
    let pipe_slot: Arc<Mutex<Option<pipe_server::PipeServer>>> = Arc::new(Mutex::new(None));

    let mut builder = tauri::Builder::default();
    if !isolated {
        builder = builder.plugin(hotkey::plugin());
    }
    let (tray_slot2, pipe_slot2) = (tray_slot.clone(), pipe_slot.clone());
    builder
        .invoke_handler(tauri::generate_handler![
            api::get_profile, api::get_shell_state, api::add_item, api::update_tile, api::delete_tile,
            api::restore_tile, api::add_group, api::rename_group, api::delete_group, api::open_tile,
            api::set_routine_steps, api::run_routine, api::list_apps, api::read_clipboard, api::update_settings,
            api::list_displays, api::get_pc_stats, api::hide, api::open_data_folder, api::copy_text,
            api::capture_layout, api::save_routine, api::delete_routine,
        ])
        .register_uri_scheme_protocol("bridge-icon", {
            let icons = home.join("icons");
            move |_ctx, req| {
                // basename only: the path can never leave the icons folder.
                let raw = req.uri().path().trim_start_matches('/');
                let name = percent_decode(raw);
                let file = Path::new(&name).file_name().map(|f| icons.join(f));
                match file.and_then(|f| std::fs::read(&f).ok().map(|b| (f, b))) {
                    Some((f, bytes)) => {
                        let mime = match f.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase().as_str() {
                            "png" => "image/png",
                            "ico" => "image/x-icon",
                            "svg" => "image/svg+xml",
                            "jpg" | "jpeg" => "image/jpeg",
                            "gif" => "image/gif",
                            "webp" => "image/webp",
                            _ => "application/octet-stream",
                        };
                        tauri::http::Response::builder().header("Content-Type", mime).body(bytes).unwrap()
                    }
                    None => tauri::http::Response::builder().status(404).body(Vec::new()).unwrap(),
                }
            }
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let wm = WindowManager::new(&handle, &home, forced, line)?;
            bridge_sys::dragdrop::install(wm.window())?;
            let shell = Arc::new(Shell { app: handle.clone(), wm: wm.clone(), isolated, on_hotkey: OnceLock::new() });
            let api = Api::new(store.clone(), home.clone(), handle.clone(), shell.clone());
            app.manage(api.clone());

            let a = api.clone();
            let _ = shell.on_hotkey.set(Arc::new(move || {
                a.shell.show();
                a.send(BridgeEvent::Command { command: UiCommand::FocusPalette });
            }));

            {
                let a = api.clone();
                wm.on_shell_change(Box::new(move |state| a.send(BridgeEvent::Shell { state })));
            }
            {
                let (a, tray_slot) = (api.clone(), tray_slot2.clone());
                store.on_change(Box::new(move |p| {
                    a.send(BridgeEvent::Profile { profile: p.clone() });
                    if let Some(t) = tray_slot.lock().unwrap().as_ref() {
                        t.refresh(p.settings.mode);
                    }
                }));
                let a = api.clone();
                store.on_notice(Box::new(move |m| a.toast_error(m)));
            }

            let settings = store.get().settings;
            let hotkey_ok = shell.apply_hotkey(&settings.hotkey);
            wm.apply(&settings);

            // CLI pipe.
            let a = api.clone();
            let server = pipe_server::start(
                &pipe,
                Arc::new(move |c| {
                    let a = a.clone();
                    Box::pin(async move {
                        let is_error = matches!(c, bridge_core::command::Command::Error { .. });
                        let r = a.handle_command(c).await;
                        if !r.ok && !is_error {
                            a.toast_error(&r.message);
                        }
                        r
                    })
                }),
            )?;
            *pipe_slot2.lock().unwrap() = Some(server);

            // Things to tell the user once the UI is up; plus a first-launch CLI command (`bridge open x`).
            {
                let (a, home, hotkey) = (api.clone(), home.clone(), settings.hotkey.clone());
                let first = parse_command(&argv);
                let cwd = cwd.clone();
                wm.on_ready(Box::new(move || {
                    if std::env::var_os("BRIDGE_TIMING").is_some() {
                        log(&home, &format!("ready after {} ms", t0.elapsed().as_millis()));
                    }
                    // Give React a moment to subscribe to events before the first toasts/commands.
                    tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                    if let Some(n) = a.store.load_notice() {
                        a.toast_error(&n);
                    }
                    if !hotkey_ok {
                        a.toast_error(&format!("{hotkey} is in use by another app. Pick another in Settings"));
                    }
                    if first != bridge_core::command::Command::Show {
                        let mut c = first;
                        if let bridge_core::command::Command::Add { cwd: ref mut c_cwd, .. } = c {
                            *c_cwd = Some(cwd);
                        }
                        let a = a.clone();
                        tauri::async_runtime::spawn(async move {
                            let r = a.handle_command(c).await;
                            if !r.ok {
                                a.toast_error(&r.message);
                            }
                        });
                    }
                    });
                }));
            }

            if !isolated {
                let (a1, a2, a3, a4, a5) = (api.clone(), api.clone(), api.clone(), api.clone(), api.clone());
                let (wm_q, h_q, pipe_q) = (wm.clone(), handle.clone(), pipe_slot2.clone());
                let s_mode = store.clone();
                let t = tray::install(
                    &handle,
                    tray::TrayActions {
                        show: Arc::new(move || a1.shell.show()),
                        start_my_day: Arc::new(move || {
                            a2.shell.show();
                            a2.send(BridgeEvent::Command { command: UiCommand::StartMyDay });
                        }),
                        set_mode: Arc::new(move |mode| {
                            a3.update_settings(serde_json::json!({ "mode": mode }));
                        }),
                        open_settings: Arc::new(move || {
                            a4.shell.show();
                            a4.send(BridgeEvent::Command { command: UiCommand::OpenSettings });
                        }),
                        open_data_folder: Arc::new(move || {
                            let _ = std::process::Command::new("explorer.exe").arg(&a5.home).spawn();
                        }),
                        quit: Arc::new(move || {
                            wm_q.allow_close();
                            hotkey::unregister(&h_q);
                            if let Some(p) = pipe_q.lock().unwrap().as_ref() {
                                p.stop();
                            }
                            h_q.exit(0);
                        }),
                        current_mode: Arc::new(move || s_mode.get().settings.mode),
                    },
                )?;
                *tray_slot2.lock().unwrap() = Some(t);
                // Warm the Start-menu app list so "+ Add" opens instantly.
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_secs(4));
                    bridge_sys::apps::prefetch_start_apps();
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Bridge could not start");
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 0 && i + 2 <= b.len() - 1 {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
