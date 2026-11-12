//! The logic behind `window.bridge` and the `bridge` CLI. Port of `src/main/api.ts`: thin glue over bridge-core
//! (profile, detection, matching, routines) and bridge-sys (launching, metadata, icons, clipboard, stats).
use bridge_core::command::{Command, HELP_TEXT};
use bridge_core::detect::{detect_kind, is_sensitive_url, Detected, FsKind};
use bridge_core::env::{expand_env, is_path_like, is_unc, win_normalize};
use bridge_core::launch_plan::plan_launch;
use bridge_core::matcher::rank;
use bridge_core::profile::{new_id, ProfileStore};
use bridge_core::profile::settings_from_value;
use bridge_core::routine::{run_routine as run_steps, StepCtx};
use bridge_core::zones::{best_zone, zone_rect};
use bridge_sys::winmgr::{self, AppWindow};
use bridge_core::stats::StatsSampler;
use bridge_core::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// What the API needs from the window/shell layer.
pub trait ShellOps: Send + Sync {
    fn state(&self) -> ShellState;
    /// Applies display settings to the window; returns the new state.
    fn apply(&self, s: &Settings) -> ShellState;
    /// Re-registers the global hotkey; false if Windows refused it.
    fn apply_hotkey(&self, accel: &str) -> bool;
    fn list_displays(&self) -> Vec<DisplayInfo>;
    fn hide(&self);
    fn show(&self);
}

/// One window found by "Capture current layout": the step it becomes, and its title for the list.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Captured {
    pub step: RoutineStep,
    pub title: String,
}

async fn list_windows() -> Vec<AppWindow> {
    tauri::async_runtime::spawn_blocking(winmgr::windows).await.unwrap_or_default()
}

pub struct Api {
    pub store: Arc<ProfileStore>,
    pub home: PathBuf,
    pub dry: bool,
    pub app: AppHandle,
    pub shell: Arc<dyn ShellOps>,
    routine_running: AtomicBool,
    /// Where a deleted tile sat in each routine, so Undo can put it back exactly.
    deleted_slots: Mutex<HashMap<String, Vec<(String, usize)>>>,
    stats: Mutex<StatsSampler>,
}

const NOT_ADDABLE: &str = "That doesn't look like a link, app, file or folder";
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

enum Detect {
    Found(Detected),
    /// Dry run on a network path: not probed while typing.
    Network,
    None,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn same_target(a: &str, b: &str) -> bool {
    let n = |s: &str| s.trim().to_lowercase().trim_end_matches('/').to_string();
    n(a) == n(b)
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(t);
    t.trim().to_string()
}

/// Async disk check with a timeout, so an unreachable network share can never freeze anything.
async fn probe(p: String) -> Option<FsKind> {
    let check = tauri::async_runtime::spawn_blocking(move || {
        let m = std::fs::metadata(expand_env(&p)).ok()?;
        if m.is_dir() { Some(FsKind::Dir) } else if m.is_file() { Some(FsKind::File) } else { None }
    });
    tokio::time::timeout(PROBE_TIMEOUT, check).await.ok()?.ok()?
}

async fn detect(input: &str, dry_run: bool) -> Detect {
    let s = unquote(input);
    if !is_path_like(&s) {
        return detect_kind(input, &|_| None).map_or(Detect::None, Detect::Found);
    }
    if dry_run && is_unc(&s) {
        return Detect::Network;
    }
    let found = probe(win_normalize(&s)).await;
    detect_kind(input, &move |_| found).map_or(Detect::None, Detect::Found)
}

/// `shell:…` and protocol targets have no file icon.
fn has_file_icon(target: &str) -> bool {
    let t = target.to_lowercase();
    if t.starts_with("shell:") {
        return false;
    }
    match t.find(':') {
        Some(i) if i >= 2 => {
            let scheme = &t[..i];
            let ok_scheme = scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
            !(ok_scheme && !t[i + 1..].starts_with('\\'))
        }
        _ => true,
    }
}

impl Api {
    pub fn new(store: Arc<ProfileStore>, home: PathBuf, app: AppHandle, shell: Arc<dyn ShellOps>) -> Arc<Api> {
        Arc::new(Api {
            store,
            dry: std::env::var("BRIDGE_DRY_LAUNCH").as_deref() == Ok("1"),
            home,
            app,
            shell,
            routine_running: AtomicBool::new(false),
            deleted_slots: Mutex::new(HashMap::new()),
            stats: Mutex::new(StatsSampler::new()),
        })
    }

    pub fn send(&self, e: BridgeEvent) {
        let _ = self.app.emit("bridge:event", e);
    }

    pub fn toast_error(&self, message: &str) {
        self.send(BridgeEvent::Toast { message: message.to_string(), tone: ToastTone::Error });
    }

    fn icons_dir(&self) -> PathBuf {
        let d = self.home.join("icons");
        let _ = std::fs::create_dir_all(&d);
        d
    }

    fn group_name(p: &Profile, id: &str) -> String {
        p.groups.iter().find(|g| g.id == id).map_or_else(|| "Bridge".into(), |g| g.name.clone())
    }

    /// Background enrichment after a tile is saved: page title + favicon, or a file/app icon. Never blocks adding.
    fn enrich(self: &Arc<Self>, tile: Tile, name_given: bool) {
        let api = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut name: Option<String> = None;
            let mut icon: Option<String> = None;
            if tile.kind == TileKind::Url && !is_sensitive_url(&tile.target) {
                let meta = bridge_sys::meta::fetch_url_meta(&tile.target, &api.icons_dir()).await;
                if !name_given {
                    name = meta.title.map(|t| t.chars().take(60).collect());
                }
                icon = meta.icon_file;
            } else if tile.kind != TileKind::Url && has_file_icon(&tile.target) {
                let (path, dir) = (PathBuf::from(expand_env(&tile.target)), api.icons_dir());
                icon = tauri::async_runtime::spawn_blocking(move || bridge_sys::icons::icon_for_path(&path, &dir))
                    .await
                    .ok()
                    .flatten();
            }
            if name.is_none() && icon.is_none() {
                return;
            }
            let _ = api.store.update(|p| {
                if let Some(t) = p.tiles.iter_mut().find(|x| x.id == tile.id) {
                    if icon.is_some() {
                        t.icon = icon.clone();
                    }
                    // Only rename if the user hasn't renamed it in the meantime.
                    if let Some(n) = name.filter(|_| t.name == tile.name) {
                        t.name = n;
                    }
                }
            });
        });
    }

    pub async fn open_tile(self: &Arc<Self>, id: &str) -> Res<()> {
        let Some(tile) = self.store.get().tiles.into_iter().find(|t| t.id == id) else {
            return Res::fail("That tile is gone. It may have been deleted.");
        };
        let (home, dry, t) = (self.home.clone(), self.dry, tile.clone());
        let r = tauri::async_runtime::spawn_blocking(move || {
            let edge = bridge_sys::launch::edge_path();
            bridge_sys::launch::execute(plan_launch(&t, edge.as_deref()), &t, &home, dry)
        })
        .await
        .unwrap_or_else(|_| Res::fail(format!("Couldn't open {}.", tile.name)));
        if r.ok {
            let _ = self.store.update(|p| {
                if let Some(t) = p.tiles.iter_mut().find(|x| x.id == id) {
                    t.last_opened_at = Some(now_iso());
                    t.open_count += 1;
                }
            });
        }
        self.send(BridgeEvent::Launched { tile_id: id.to_string(), ok: r.ok, message: r.message.clone() });
        r
    }

    pub async fn run_routine(self: &Arc<Self>, routine_id: &str) -> Res<()> {
        let p = self.store.get();
        let Some(r) = p.routines.iter().find(|x| x.id == routine_id).cloned() else {
            return Res::fail("That routine is gone.");
        };
        if self.routine_running.swap(true, Ordering::SeqCst) {
            return Res::fail(format!("{} is already running", r.name));
        }
        let api = self.clone();
        let rid = r.id.clone();
        let res = run_steps(
            &r,
            &p.tiles,
            |c: StepCtx| {
                let (api, rid) = (api.clone(), rid.clone());
                async move { api.run_step(&rid, c).await }
            },
            |progress| self.send(BridgeEvent::Routine { progress }),
            Duration::from_millis(700),
        )
        .await;
        self.routine_running.store(false, Ordering::SeqCst);
        res
    }

    /// One routine/workspace step: reuse the app's window if it's already open (no double opens), otherwise open
    /// the tile, wait for its new window and put it in its zone. Learns the window's app on the first run.
    async fn run_step(self: &Arc<Self>, routine_id: &str, c: StepCtx) -> Res<()> {
        let step = c.step;
        if !self.dry {
            if let Some(h) = &step.hint {
                let all = list_windows().await;
                if let Some(w) = winmgr::find(&all, h) {
                    let hwnd = w.hwnd;
                    match &step.place {
                        Some(pl) => {
                            self.place_window(hwnd, pl).await;
                        }
                        None => winmgr::focus(hwnd),
                    }
                    let name = c.tile.as_ref().map_or_else(|| h.exe.trim_end_matches(".exe").to_string(), |t| t.name.clone());
                    let msg = format!("{name} was already open");
                    self.send(BridgeEvent::Launched { tile_id: step.tile_id.clone(), ok: true, message: msg.clone() });
                    return Res::ok(msg);
                }
            }
        }
        let Some(tile) = c.tile else {
            return Res::fail("isn't open");
        };
        let wants_window = !self.dry && (step.place.is_some() || (step.hint.is_none() && tile.kind != TileKind::Url));
        let before: std::collections::HashSet<isize> =
            if wants_window { list_windows().await.into_iter().map(|w| w.hwnd).collect() } else { Default::default() };
        let r = self.open_tile(&tile.id).await;
        if !r.ok || !wants_window {
            return r;
        }
        // Wait (up to 15 s) for the window this launch created.
        let expected = step.hint.as_ref().map(|h| h.exe.to_lowercase()).or_else(|| match tile.kind {
            TileKind::Url => Some("msedge.exe".to_string()),
            _ => None,
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut found: Option<AppWindow> = None;
        while std::time::Instant::now() < deadline {
            found = list_windows()
                .await
                .into_iter()
                .find(|w| !before.contains(&w.hwnd) && expected.as_ref().is_none_or(|e| &w.exe == e));
            if found.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        let Some(w) = found else { return r };
        if let Some(pl) = &step.place {
            // Apps often resize themselves right after they appear; let them settle, then place.
            tokio::time::sleep(Duration::from_millis(400)).await;
            self.place_window(w.hwnd, pl).await;
        }
        // Remember which app this step's window belongs to, so next time an open one is reused instead of doubled.
        // Not for links: every Edge window is msedge.exe.
        if step.hint.is_none() && tile.kind != TileKind::Url {
            let idx = c.index as usize;
            let _ = self.store.update(|p| {
                if let Some(s) = p.routines.iter_mut().find(|r| r.id == routine_id).and_then(|r| r.steps.get_mut(idx)) {
                    if s.tile_id == tile.id && s.hint.is_none() {
                        s.hint = Some(WindowHint { exe: w.exe.clone(), ..Default::default() });
                    }
                }
            });
        }
        r
    }

    async fn place_window(&self, hwnd: isize, pl: &Placement) -> bool {
        let pl = pl.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mons = winmgr::monitors();
            let Some(m) = mons.iter().find(|m| m.id == pl.display_id) else { return false };
            let Some(rect) = zone_rect(&m.work, &pl.zone) else { return false };
            winmgr::place(hwnd, rect, pl.zone == "full")
        })
        .await
        .unwrap_or(false)
    }

    /// The app windows on screen right now, as window-only steps placed where they are (for a new workspace).
    pub async fn capture_layout(&self) -> Vec<Captured> {
        let tiles = self.store.get().tiles;
        tauri::async_runtime::spawn_blocking(move || {
            let mons = winmgr::monitors();
            let all = winmgr::windows();
            let mut seen: HashMap<String, u32> = HashMap::new();
            let mut out = Vec::new();
            for w in all.iter().filter(|w| w.exe != "bridge.exe" && !w.minimized) {
                let nth = seen.entry(w.exe.clone()).or_insert(0);
                let index = *nth;
                *nth += 1;
                let Some(m) = mons.iter().find(|m| m.id == w.monitor_id) else { continue };
                let zone = best_zone(&m.work, &w.rect).to_string();
                // A tile that launches this app, if there is one (matched by the target's file name).
                let tile_id = tiles
                    .iter()
                    .find(|t| {
                        let target = expand_env(&t.target).to_lowercase();
                        t.kind == TileKind::App && target.rsplit(['\\', '/']).next() == Some(w.exe.as_str())
                    })
                    .map(|t| t.id.clone())
                    .unwrap_or_default();
                out.push(Captured {
                    title: w.title.clone(),
                    step: RoutineStep {
                        tile_id,
                        place: Some(Placement { display_id: m.id, zone }),
                        hint: Some(WindowHint { exe: w.exe.clone(), title: None, nth: (index > 0).then_some(index) }),
                    },
                });
            }
            out
        })
        .await
        .unwrap_or_default()
    }

    pub fn save_routine(&self, mut r: Routine) -> Res<Routine> {
        r.name = r.name.trim().to_string();
        if r.name.is_empty() {
            return Res::fail("Give it a name");
        }
        let p = self.store.get();
        if p.routines.iter().any(|x| x.id != r.id && x.name.eq_ignore_ascii_case(&r.name)) {
            return Res::fail(format!("There's already a routine called {}", r.name));
        }
        if r.id.is_empty() {
            r.id = new_id("r");
        }
        let saved = r.clone();
        match self.store.update(|q| match q.routines.iter_mut().find(|x| x.id == saved.id) {
            Some(x) => *x = saved,
            None => q.routines.push(saved),
        }) {
            Ok(next) => next
                .routines
                .into_iter()
                .find(|x| x.id == r.id)
                .map_or_else(|| Res::fail("Couldn't save that"), |x| Res::ok_with("Saved", x)),
            Err(e) => Res::fail(e),
        }
    }

    pub fn delete_routine(&self, id: &str) -> Res<()> {
        if id == START_MY_DAY_ID {
            return Res::fail("Start my day can't be deleted. Remove its steps instead.");
        }
        let _ = self.store.update(|p| p.routines.retain(|r| r.id != id));
        Res::ok("Deleted")
    }

    pub fn list_displays(&self) -> Vec<DisplayInfo> {
        let mons = winmgr::monitors();
        let mut ds = self.shell.list_displays();
        for d in &mut ds {
            if let Some(m) = mons.iter().find(|m| m.id == d.id) {
                d.bounds = m.bounds;
                d.work = m.work;
            }
        }
        ds
    }

    pub async fn add_item(self: &Arc<Self>, req: AddRequest) -> Res<Tile> {
        let p = self.store.get();
        if !p.groups.iter().any(|g| g.id == req.group_id) {
            return Res::fail("Pick a group first");
        }
        let dry_run = req.dry_run.unwrap_or(false);
        let det = match detect(&req.input, dry_run).await {
            Detect::Network => return Res::ok("Looks like: Network path. Press Add to check it"),
            Detect::None => return Res::fail(NOT_ADDABLE),
            Detect::Found(d) => d,
        };
        let given = req.name.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let name = given.map_or_else(|| det.name.clone(), str::to_string);
        let tile = Tile {
            id: if dry_run { String::new() } else { new_id("t") },
            group_id: req.group_id.clone(),
            name,
            kind: det.kind,
            target: det.target.clone(),
            args: None,
            keyword: None,
            subtitle: Some(det.subtitle.clone()),
            icon: None,
            created_at: now_iso(),
            last_opened_at: None,
            open_count: 0,
        };
        let kind_label = match det.kind {
            TileKind::Url => "Link",
            TileKind::App => "App",
            TileKind::Folder => "Folder",
            TileKind::File => "File",
        };
        if dry_run {
            let what = if det.subtitle == kind_label { tile.name.clone() } else { det.subtitle.clone() };
            return Res::ok_with(format!("Looks like: {kind_label} · {what}"), tile);
        }
        if let Some(dup) = p.tiles.iter().find(|t| same_target(&t.target, &det.target)) {
            return Res::fail(format!("That's already here: {} in {}", dup.name, Self::group_name(&p, &dup.group_id)));
        }
        let saved = tile.clone();
        if let Err(e) = self.store.update(|q| q.tiles.push(saved)) {
            return Res::fail(e);
        }
        self.enrich(tile.clone(), given.is_some());
        Res::ok_with(format!("Added {} to {}", tile.name, Self::group_name(&p, &req.group_id)), tile)
    }

    pub async fn update_tile(self: &Arc<Self>, id: &str, mut patch: TilePatch) -> Res<Tile> {
        let p = self.store.get();
        let Some(old) = p.tiles.iter().find(|t| t.id == id).cloned() else {
            return Res::fail("That tile is gone.");
        };
        if let Some(n) = &patch.name {
            let n = n.trim().to_string();
            if n.is_empty() {
                return Res::fail("Give it a name");
            }
            patch.name = Some(n);
        }
        if let Some(g) = &patch.group_id {
            if !p.groups.iter().any(|x| &x.id == g) {
                return Res::fail("That group is gone.");
            }
        }
        // keyword: Some("") means "clear it".
        let keyword_change = patch.keyword.take().map(|k| {
            let k = k.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join("-");
            (!k.is_empty()).then_some(k)
        });
        if let Some(Some(k)) = &keyword_change {
            if let Some(clash) = p.tiles.iter().find(|t| t.id != id && t.keyword.as_deref() == Some(k)) {
                return Res::fail(format!("\"{k}\" is already used by {}", clash.name));
            }
        }
        let apply = |t: &mut Tile, patch: &TilePatch| {
            if let Some(v) = &patch.name { t.name = v.clone() }
            if let Some(v) = &patch.group_id { t.group_id = v.clone() }
            if let Some(v) = &patch.subtitle { t.subtitle = Some(v.clone()) }
            if let Some(k) = &keyword_change { t.keyword = k.clone() }
        };
        // Only re-check the target when it really changed, so renaming an offline folder tile (off VPN) still works.
        let new_target = patch.target.take().filter(|t| t.trim() != old.target);
        if let Some(target) = new_target {
            let det = match detect(&target, false).await {
                Detect::Found(d) => d,
                _ => return Res::fail(NOT_ADDABLE),
            };
            let next = self.store.update(|q| {
                if let Some(t) = q.tiles.iter_mut().find(|x| x.id == id) {
                    apply(t, &patch);
                    t.target = det.target.clone();
                    t.kind = det.kind;
                    t.subtitle = Some(det.subtitle.clone());
                    t.icon = None;
                }
            });
            return match next.ok().and_then(|n| n.tiles.into_iter().find(|t| t.id == id)) {
                Some(saved) => {
                    self.enrich(saved.clone(), true);
                    Res::ok_with("Saved", saved)
                }
                None => Res::fail("That tile is gone."),
            };
        }
        match self.store.update(|q| {
            if let Some(t) = q.tiles.iter_mut().find(|x| x.id == id) {
                apply(t, &patch)
            }
        }) {
            Ok(n) => n.tiles.into_iter().find(|t| t.id == id).map_or_else(|| Res::fail("That tile is gone."), |t| Res::ok_with("Saved", t)),
            Err(e) => Res::fail(e),
        }
    }

    pub fn delete_tile(&self, id: &str) -> Res<Tile> {
        let Some(tile) = self.store.get().tiles.into_iter().find(|t| t.id == id) else {
            return Res::fail("That tile is gone.");
        };
        let mut slots = Vec::new();
        let _ = self.store.update(|p| {
            p.tiles.retain(|t| t.id != id);
            for r in &mut p.routines {
                for (i, s) in r.steps.iter().enumerate() {
                    if s.tile_id == id {
                        slots.push((r.id.clone(), i));
                    }
                }
                r.steps.retain(|s| s.tile_id != id);
            }
        });
        self.deleted_slots.lock().unwrap().insert(id.to_string(), slots);
        Res::ok_with(format!("Deleted {}", tile.name), tile)
    }

    pub fn restore_tile(&self, tile: Tile) -> Res<()> {
        if self.store.get().tiles.iter().any(|t| t.id == tile.id) {
            return Res::ok("Already back");
        }
        let slots = self.deleted_slots.lock().unwrap().remove(&tile.id).unwrap_or_default();
        let name = tile.name.clone();
        let _ = self.store.update(|p| {
            let mut t = tile;
            if !p.groups.iter().any(|g| g.id == t.group_id) {
                if let Some(g) = p.groups.first() {
                    t.group_id = g.id.clone();
                }
            }
            let id = t.id.clone();
            p.tiles.push(t);
            for (rid, index) in &slots {
                if let Some(r) = p.routines.iter_mut().find(|r| &r.id == rid) {
                    let at = (*index).min(r.steps.len());
                    r.steps.insert(at, RoutineStep::tile(id.clone()));
                }
            }
        });
        Res::ok(format!("Restored {name}"))
    }

    pub fn add_group(&self, name: &str) -> Res<Group> {
        let n = name.trim();
        let g = Group { id: new_id("g"), name: if n.is_empty() { "New group".into() } else { n.into() } };
        let saved = g.clone();
        let _ = self.store.update(|p| p.groups.push(saved));
        Res::ok_with(format!("Added {}", g.name), g)
    }

    pub fn rename_group(&self, id: &str, name: &str) -> Res<()> {
        if name.trim().is_empty() {
            return Res::fail("Give it a name");
        }
        let _ = self.store.update(|p| {
            if let Some(g) = p.groups.iter_mut().find(|g| g.id == id) {
                g.name = name.trim().to_string();
            }
        });
        Res::ok("Renamed")
    }

    pub fn delete_group(&self, id: &str) -> Res<()> {
        let p = self.store.get();
        if p.tiles.iter().any(|t| t.group_id == id) {
            return Res::fail("Move or delete its tiles first");
        }
        if p.groups.len() <= 1 {
            return Res::fail("Bridge needs at least one group");
        }
        let _ = self.store.update(|q| q.groups.retain(|g| g.id != id));
        Res::ok("Group deleted")
    }

    pub fn set_routine_steps(&self, routine_id: &str, tile_ids: Vec<String>) -> Res<()> {
        let p = self.store.get();
        if !p.routines.iter().any(|r| r.id == routine_id) {
            return Res::fail("That routine is gone.");
        }
        // Keep each kept tile's placement and window hint.
        let old = p.routines.iter().find(|r| r.id == routine_id).map(|r| r.steps.clone()).unwrap_or_default();
        let valid: Vec<RoutineStep> = tile_ids
            .into_iter()
            .filter(|id| p.tiles.iter().any(|t| &t.id == id))
            .map(|id| old.iter().find(|s| s.tile_id == id).cloned().unwrap_or_else(|| RoutineStep::tile(id)))
            .collect();
        let _ = self.store.update(|q| {
            if let Some(r) = q.routines.iter_mut().find(|r| r.id == routine_id) {
                r.steps = valid;
            }
        });
        Res::ok("Saved")
    }

    /// `patch` is a partial Settings object from the renderer, merged into the current settings; every field is
    /// re-validated (bad values fall back to defaults).
    pub fn update_settings(&self, patch: serde_json::Value) -> Res<ShellState> {
        let before = self.store.get().settings;
        let Some(obj) = patch.as_object() else { return Res::fail("Bridge could not read those settings") };
        if let Some(h) = obj.get("hotkey").and_then(|v| v.as_str()).filter(|h| *h != before.hotkey) {
            if !self.shell.apply_hotkey(h) {
                self.shell.apply_hotkey(&before.hotkey); // keep the old one working
                let mut state = self.shell.state();
                state.hotkey = h.to_string();
                state.hotkey_ok = false;
                return Res { ok: false, message: format!("{h} is taken by another app. Try a different one."), data: Some(state) };
            }
        }
        let mut merged = serde_json::to_value(&before).unwrap_or_default();
        if let Some(m) = merged.as_object_mut() {
            for (k, v) in obj {
                m.insert(k.clone(), v.clone());
            }
        }
        let next = settings_from_value(&merged);
        if let Err(e) = self.store.update(|p| p.settings = next.clone()) {
            return Res::fail(e);
        }
        let state = if obj.contains_key("mode") || obj.contains_key("pinnedDisplayId") {
            self.shell.apply(&next)
        } else {
            self.shell.state()
        };
        self.send(BridgeEvent::Shell { state: state.clone() });
        Res::ok_with("Saved", state)
    }

    pub fn pc_stats(&self) -> PcStats {
        let (idle, total, free, all) = bridge_sys::stats::read_raw();
        self.stats.lock().unwrap().sample(idle, total, free, all)
    }

    /// `bridge …` from PowerShell (pipe) or a second launch.
    pub async fn handle_command(self: &Arc<Self>, c: Command) -> Res<()> {
        let p = self.store.get();
        match c {
            Command::Show => {
                self.shell.show();
                Res::ok("Bridge is up")
            }
            Command::Help => Res::ok(HELP_TEXT),
            Command::Error { message } => Res::fail(message),
            Command::Start => {
                self.shell.show();
                self.run_routine(START_MY_DAY_ID).await
            }
            Command::Open { query } => {
                let hit = rank(&query, &p.tiles, |t| {
                    vec![t.keyword.clone().unwrap_or_default(), t.name.clone(), t.subtitle.clone().unwrap_or_default()]
                })
                .first()
                .map(|t| t.id.clone());
                match hit {
                    Some(id) => self.open_tile(&id).await,
                    None => Res::fail(format!("Nothing matches \"{query}\"")),
                }
            }
            Command::Run { query } => {
                let Some(id) = rank(&query, &p.routines, |r| vec![r.name.clone()]).first().map(|r| r.id.clone()) else {
                    return Res::fail(format!("No routine matches \"{query}\""));
                };
                self.shell.show();
                self.run_routine(&id).await
            }
            Command::Add { input, cwd } => {
                let Some(group_id) = p.groups.first().map(|g| g.id.clone()) else {
                    return Res::fail("Pick a group first");
                };
                // `bridge add report.docx` typed inside a folder means that folder's file.
                let mut input = unquote(&input);
                let relative = {
                    let l = input.to_lowercase();
                    let b = l.as_bytes();
                    !(b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
                        && !l.starts_with(['\\', '/', '%'])
                        && !l.split_once(':').is_some_and(|(s, _)| {
                            s.len() >= 1 && s.chars().next().unwrap().is_ascii_alphabetic()
                                && s.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
                        })
                };
                if let (Some(cwd), true) = (cwd, relative) {
                    let joined = Path::new(&cwd).join(&input).to_string_lossy().to_string();
                    if probe(joined.clone()).await.is_some() {
                        input = joined;
                    }
                }
                self.add_item(AddRequest { input, name: None, group_id, dry_run: None }).await.plain()
            }
        }
    }
}

// ---- Tauri commands (names = snake_case of MainApi methods; see src/renderer/bridge.ts) ------------------------

type A<'a> = tauri::State<'a, Arc<Api>>;
type R<T> = Result<T, String>;

#[tauri::command] pub fn get_profile(api: A) -> Profile { api.store.get() }
#[tauri::command] pub fn get_shell_state(api: A) -> ShellState { api.shell.state() }
#[tauri::command] pub async fn add_item(api: A<'_>, req: AddRequest) -> R<Res<Tile>> { Ok(api.add_item(req).await) }
#[tauri::command] pub async fn update_tile(api: A<'_>, id: String, patch: TilePatch) -> R<Res<Tile>> { Ok(api.update_tile(&id, patch).await) }
#[tauri::command] pub fn delete_tile(api: A, id: String) -> Res<Tile> { api.delete_tile(&id) }
#[tauri::command] pub fn restore_tile(api: A, tile: Tile) -> Res<()> { api.restore_tile(tile) }
#[tauri::command] pub fn add_group(api: A, name: String) -> Res<Group> { api.add_group(&name) }
#[tauri::command] pub fn rename_group(api: A, id: String, name: String) -> Res<()> { api.rename_group(&id, &name) }
#[tauri::command] pub fn delete_group(api: A, id: String) -> Res<()> { api.delete_group(&id) }
#[tauri::command] pub async fn open_tile(api: A<'_>, id: String) -> R<Res<()>> { Ok(api.inner().open_tile(&id).await) }
#[tauri::command] pub fn set_routine_steps(api: A, routine_id: String, tile_ids: Vec<String>) -> Res<()> { api.set_routine_steps(&routine_id, tile_ids) }
#[tauri::command] pub async fn run_routine(api: A<'_>, routine_id: String) -> R<Res<()>> { Ok(api.inner().run_routine(&routine_id).await) }
#[tauri::command] pub async fn list_apps() -> R<Vec<AppEntry>> {
    Ok(tauri::async_runtime::spawn_blocking(bridge_sys::apps::list_start_apps).await.unwrap_or_default())
}
#[tauri::command] pub async fn read_clipboard() -> R<String> {
    Ok(tauri::async_runtime::spawn_blocking(bridge_sys::clipboard::read_input).await.unwrap_or_default())
}
#[tauri::command] pub fn update_settings(api: A, patch: serde_json::Value) -> Res<ShellState> { api.update_settings(patch) }
#[tauri::command] pub fn list_displays(api: A) -> Vec<DisplayInfo> { api.list_displays() }
#[tauri::command] pub async fn capture_layout(api: A<'_>) -> R<Vec<Captured>> { Ok(api.capture_layout().await) }
#[tauri::command] pub fn save_routine(api: A, routine: Routine) -> Res<Routine> { api.save_routine(routine) }
#[tauri::command] pub fn delete_routine(api: A, id: String) -> Res<()> { api.delete_routine(&id) }
#[tauri::command] pub fn get_pc_stats(api: A) -> PcStats { api.pc_stats() }
#[tauri::command] pub fn hide(api: A) { api.shell.hide() }
#[tauri::command] pub fn open_data_folder(api: A) {
    let tile = Tile {
        id: String::new(), group_id: String::new(), name: "Bridge data".into(), kind: TileKind::Folder,
        target: api.home.to_string_lossy().into(), args: None, keyword: None, subtitle: None, icon: None,
        created_at: String::new(), last_opened_at: None, open_count: 0,
    };
    let _ = bridge_sys::launch::execute(plan_launch(&tile, None), &tile, &api.home, api.dry);
}
#[tauri::command] pub fn copy_text(text: String) { bridge_sys::clipboard::write_text(&text); }
