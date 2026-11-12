//! Profile validation + normalization. Port of `src/main/profile/schema.ts` (zod v4 rules, by hand).
//!
//! Lenient by design: a hand-edited profile.json should still open, so bad items are dropped or repaired one by one.
//! Only a structurally broken file (not an object, lists that aren't arrays) is an error; the store treats that as
//! "corrupt".
use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::types::{DisplayMode, Group, Placement, Profile, Routine, RoutineStep, Settings, Tile, TileKind, WindowHint, START_MY_DAY_ID};

/// `<prefix>-` + 8 random base36 chars, e.g. `t-k3x9q0ab`.
pub fn new_id(prefix: &str) -> String {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("OS random source unavailable");
    let s: String = bytes.iter().map(|b| std::char::from_digit(u32::from(b % 36), 36).unwrap_or('0')).collect();
    format!("{prefix}-{s}")
}

/// Validate and normalize anything read from disk into a well-formed Profile. Errors only for broken structure.
pub fn parse_profile(raw: Value) -> Result<Profile, String> {
    parse_profile_report(raw).map(|(p, _)| p)
}

/// JS `String.prototype.trim` whitespace: Unicode White_Space minus U+0085, plus U+FEFF.
fn js_ws(c: char) -> bool {
    c == '\u{FEFF}' || (c != '\u{85}' && c.is_whitespace())
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(js_ws)
}

/// `z.string().optional().catch(undefined)`
fn opt_str(o: &Map<String, Value>, key: &str) -> Option<String> {
    o.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// `z.string().trim().min(1).optional().catch(undefined)` (the output is trimmed).
fn id_of(o: &Map<String, Value>, key: &str) -> Option<String> {
    o.get(key).and_then(Value::as_str).map(js_trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// TS `present()`: the trimmed string when it isn't blank.
fn present(s: Option<&String>) -> Option<String> {
    s.map(|s| js_trim(s)).filter(|t| !t.is_empty()).map(str::to_owned)
}

fn kind_of(v: Option<&Value>) -> Option<TileKind> {
    match v.and_then(Value::as_str)? {
        "url" => Some(TileKind::Url),
        "app" => Some(TileKind::App),
        "folder" => Some(TileKind::Folder),
        "file" => Some(TileKind::File),
        _ => None,
    }
}

/// `z.number().int().min(0).catch(0)`; values that don't fit a u32 also become 0.
fn open_count_of(v: Option<&Value>) -> u32 {
    match v.and_then(Value::as_f64) {
        Some(n) if n.fract() == 0.0 && n >= 0.0 && n <= f64::from(u32::MAX) => n as u32,
        _ => 0,
    }
}

/// `z.number().nullable().catch(null)`; ids are integers, so a fractional number also becomes null.
fn display_id_of(v: Option<&Value>) -> Option<i64> {
    let n = v?;
    if let Some(i) = n.as_i64() {
        return Some(i);
    }
    match n.as_f64() {
        Some(f) if f.fract() == 0.0 && f >= i64::MIN as f64 && f < i64::MAX as f64 => Some(f as i64),
        _ => None,
    }
}

struct TileIn {
    id: Option<String>,
    group_id: Option<String>,
    name: Option<String>,
    kind: TileKind,
    target: String,
    args: Option<Vec<String>>,
    keyword: Option<String>,
    subtitle: Option<String>,
    icon: Option<String>,
    created_at: Option<String>,
    last_opened_at: Option<String>,
    open_count: u32,
}

/// `tileSchema.safeParse`: None = the item is unreadable (not an object, bad kind, or no target).
fn tile_schema(v: &Value) -> Option<TileIn> {
    let o = v.as_object()?;
    let kind = kind_of(o.get("kind"));
    let target = o.get("target").and_then(Value::as_str).map(js_trim).filter(|s| !s.is_empty());
    let (kind, target) = (kind?, target?.to_owned());
    let args = o
        .get("args")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().map(|x| x.as_str().map(str::to_owned)).collect::<Option<Vec<_>>>());
    Some(TileIn {
        id: id_of(o, "id"),
        group_id: opt_str(o, "groupId"),
        name: opt_str(o, "name"),
        kind,
        target,
        args,
        keyword: opt_str(o, "keyword"),
        subtitle: opt_str(o, "subtitle"),
        icon: opt_str(o, "icon"),
        created_at: opt_str(o, "createdAt"),
        last_opened_at: opt_str(o, "lastOpenedAt"),
        open_count: open_count_of(o.get("openCount")),
    })
}

/// A routine step: a known tile, and/or a window hint (window-only steps). Placement/hint are kept only if valid.
fn step_schema(v: &Value, tile_ids: &HashSet<String>) -> Option<RoutineStep> {
    let o = v.as_object()?;
    let tile_id = o.get("tileId").and_then(Value::as_str).unwrap_or("").to_string();
    let place = o.get("place").and_then(|p| serde_json::from_value::<Placement>(p.clone()).ok())
        .filter(|p| crate::zones::ZONES.contains(&p.zone.as_str()));
    let hint = o.get("hint").and_then(|h| serde_json::from_value::<WindowHint>(h.clone()).ok())
        .filter(|h| !h.exe.trim().is_empty());
    let tile_ok = !tile_id.is_empty() && tile_ids.contains(&tile_id);
    if !tile_ok && hint.is_none() {
        return None;
    }
    Some(RoutineStep { tile_id: if tile_ok { tile_id } else { String::new() }, place, hint })
}

/// Settings with every field validated on its own (bad or missing → default). Also used to apply a partial patch:
/// merge the patch keys into the current settings JSON and run it through here.
pub fn settings_from_value(v: &Value) -> Settings {
    settings_schema(Some(v))
}

fn settings_schema(v: Option<&Value>) -> Settings {
    let Some(o) = v.and_then(Value::as_object) else { return Settings::default() };
    let d = Settings::default();
    let one_of = |key: &str, allowed: &[&str], fallback: &str| -> String {
        o.get(key).and_then(Value::as_str).filter(|s| allowed.contains(s)).unwrap_or(fallback).to_string()
    };
    let num = |key: &str, lo: u64, hi: u64, fallback: u8| -> u8 {
        o.get(key).and_then(Value::as_u64).map_or(fallback, |n| n.clamp(lo, hi) as u8)
    };
    let hex = |s: &str| s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit());
    Settings {
        theme: one_of("theme", &["command", "ember", "mono", "contrast"], &d.theme),
        accent: o.get("accent").and_then(Value::as_str).filter(|s| hex(s)).map(str::to_lowercase),
        glass: num("glass", 0, 100, d.glass),
        density: one_of("density", &["comfortable", "compact"], &d.density),
        font_scale: num("fontScale", 85, 130, d.font_scale),
        fps: match o.get("fps").and_then(Value::as_u64) {
            Some(n @ (15 | 24 | 30 | 60)) => n as u8,
            _ => d.fps,
        },
        quality: one_of("quality", &["low", "medium", "high"], &d.quality),
        widgets: match o.get("widgets").and_then(Value::as_array) {
            Some(a) => {
                let mut out: Vec<String> = Vec::new();
                for w in a.iter().filter_map(Value::as_str) {
                    if ["routine", "recent", "pc", "focus", "notes", "clock"].contains(&w) && !out.iter().any(|x| x == w) {
                        out.push(w.to_string());
                    }
                }
                out
            }
            None => d.widgets.clone(),
        },
        hotkey: o
            .get("hotkey")
            .and_then(Value::as_str)
            .map(js_trim)
            .filter(|s| !s.is_empty())
            .map_or(d.hotkey, str::to_owned),
        mode: match o.get("mode").and_then(Value::as_str) {
            Some("auto") => DisplayMode::Auto,
            Some("pinned") => DisplayMode::Pinned,
            Some("summon") => DisplayMode::Summon,
            _ => d.mode,
        },
        pinned_display_id: display_id_of(o.get("pinnedDisplayId")),
        calm_motion: o.get("calmMotion").and_then(Value::as_bool).unwrap_or(false),
    }
}

/// `rootSchema` list field: missing → empty; present but not an array → broken structure.
fn list<'a>(root: &'a Map<String, Value>, key: &str) -> Result<&'a [Value], String> {
    match root.get(key) {
        None => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        Some(_) => Err(format!("Profile field \"{key}\" is not a list")),
    }
}

/// Like `parse_profile`, and also counts items that were unreadable and left out (so callers can keep the original).
pub fn parse_profile_report(raw: Value) -> Result<(Profile, usize), String> {
    let mut dropped = 0usize;
    let Some(root) = raw.as_object() else { return Err("Profile is not an object".into()) };
    let (raw_groups, raw_tiles, raw_routines) = (list(root, "groups")?, list(root, "tiles")?, list(root, "routines")?);

    let mut groups: Vec<Group> = Vec::new();
    let mut group_ids: HashSet<String> = HashSet::new();
    for g in raw_groups {
        let Some(o) = g.as_object() else {
            dropped += 1;
            continue;
        };
        let gid = id_of(o, "id").unwrap_or_else(|| new_id("g"));
        if !group_ids.insert(gid.clone()) {
            continue;
        }
        groups.push(Group { id: gid, name: present(opt_str(o, "name").as_ref()).unwrap_or_else(|| "Group".into()) });
    }
    if groups.is_empty() {
        groups.push(Group { id: "g-work".into(), name: "Work".into() });
        group_ids.insert("g-work".into());
    }

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    let mut tiles: Vec<Tile> = Vec::new();
    let mut tile_ids: HashSet<String> = HashSet::new();
    for t in raw_tiles {
        let Some(d) = tile_schema(t) else {
            dropped += 1;
            continue;
        };
        let tid = d.id.clone().unwrap_or_else(|| new_id("t"));
        if !tile_ids.insert(tid.clone()) {
            continue;
        }
        let group_id = match d.group_id {
            Some(g) if group_ids.contains(&g) => g,
            _ => groups[0].id.clone(),
        };
        let keep_raw = |s: Option<String>| s.filter(|v| present(Some(v)).is_some());
        tiles.push(Tile {
            id: tid,
            group_id,
            name: present(d.name.as_ref()).unwrap_or_else(|| d.target.clone()),
            kind: d.kind,
            target: d.target,
            args: d.args.filter(|a| !a.is_empty()),
            keyword: present(d.keyword.as_ref()),
            subtitle: keep_raw(d.subtitle),
            icon: keep_raw(d.icon),
            created_at: present(d.created_at.as_ref()).unwrap_or_else(|| now.clone()),
            last_opened_at: keep_raw(d.last_opened_at),
            open_count: d.open_count,
        });
    }

    let mut routines: Vec<Routine> = Vec::new();
    let mut routine_ids: HashSet<String> = HashSet::new();
    for x in raw_routines {
        let Some(o) = x.as_object() else {
            dropped += 1;
            continue;
        };
        let rid = id_of(o, "id").unwrap_or_else(|| new_id("r"));
        if !routine_ids.insert(rid.clone()) {
            continue;
        }
        let steps = o
            .get("steps")
            .and_then(Value::as_array)
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter_map(|s| step_schema(s, &tile_ids))
            .collect();
        let fallback = if rid == START_MY_DAY_ID { "Start my day" } else { "Routine" };
        let name = present(opt_str(o, "name").as_ref()).unwrap_or_else(|| fallback.into());
        routines.push(Routine { id: rid, name, steps });
    }
    if !routine_ids.contains(START_MY_DAY_ID) {
        routines.insert(0, Routine { id: START_MY_DAY_ID.into(), name: "Start my day".into(), steps: vec![] });
    }

    let settings = settings_schema(root.get("settings"));
    Ok((Profile { version: 1, groups, tiles, routines, settings }, dropped))
}
