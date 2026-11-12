//! Port of `tests/unit/store.test.ts` (same cases, same assertions), plus a few extra normalization checks.
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bridge_core::profile::{new_id, parse_profile, parse_profile_report, ProfileStore};
use bridge_core::types::{DisplayMode, START_MY_DAY_ID};
use regex::Regex;
use serde_json::{json, Value};

fn starter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/starter-profile.json")
}

fn temp_home() -> tempfile::TempDir {
    tempfile::Builder::new().prefix("bridge-store-").tempdir().unwrap()
}

fn profile_path(home: &Path) -> PathBuf {
    home.join("profile.json")
}

fn read_json(p: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

fn read_text(p: &Path) -> String {
    fs::read_to_string(p).unwrap()
}

fn names(home: &Path) -> Vec<String> {
    fs::read_dir(home).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect()
}

fn copies(home: &Path, label: &str) -> Vec<String> {
    let prefix = format!("profile.{label}-");
    names(home).into_iter().filter(|f| f.starts_with(&prefix)).collect()
}

fn write(p: &Path, text: &str) {
    fs::write(p, text).unwrap();
}

// ---------------------------------------------------------------- ProfileStore

#[test]
fn first_run_creates_profile_json_from_the_starter() {
    let dir = temp_home();
    let home = dir.path();
    let store = ProfileStore::open(home, &starter()).unwrap();
    assert!(profile_path(home).exists());
    assert_eq!(store.load_notice(), None);
    let p = store.get();
    assert!(p.tiles.iter().any(|t| t.name == "Calculator"));
    assert!(p.routines.iter().any(|r| r.id == START_MY_DAY_ID));
    assert_eq!(read_json(&profile_path(home))["tiles"].as_array().unwrap().len(), p.tiles.len());
}

#[test]
fn first_run_generates_ids_for_starter_items_that_lack_them() {
    let dir = temp_home();
    let home = dir.path();
    let starter = home.join("starter.json");
    write(
        &starter,
        &json!({
            "version": 1,
            "groups": [{ "name": "Work" }],
            "tiles": [{ "name": "Ex", "kind": "url", "target": "https://example.com/" }],
            "routines": [],
            "settings": {},
        })
        .to_string(),
    );
    let sub = home.join("h");
    let p = ProfileStore::open(&sub, &starter).unwrap().get();
    assert!(Regex::new(r"^g-[a-z0-9]{8}$").unwrap().is_match(&p.groups[0].id), "{}", p.groups[0].id);
    assert!(Regex::new(r"^t-[a-z0-9]{8}$").unwrap().is_match(&p.tiles[0].id), "{}", p.tiles[0].id);
    assert_eq!(p.tiles[0].group_id, p.groups[0].id);
}

#[test]
fn update_persists_and_reopening_sees_the_change() {
    let dir = temp_home();
    let home = dir.path();
    let store = ProfileStore::open(home, &starter()).unwrap();
    let out = store.update(|p| p.groups[0].name = "Changed".into()).unwrap();
    assert_eq!(out.groups[0].name, "Changed");
    assert_eq!(store.get().groups[0].name, "Changed");
    let again = ProfileStore::open(home, &starter()).unwrap();
    assert_eq!(again.get().groups[0].name, "Changed");
    assert_eq!(again.load_notice(), None);
}

#[test]
fn bak_exists_after_the_2nd_write() {
    let dir = temp_home();
    let home = dir.path();
    let store = ProfileStore::open(home, &starter()).unwrap();
    store.update(|p| p.groups[0].name = "One".into()).unwrap();
    store.update(|p| p.groups[0].name = "Two".into()).unwrap();
    let bak = home.join("profile.json.bak");
    assert!(bak.exists());
    assert_eq!(read_json(&bak)["groups"][0]["name"], "One");
    assert_eq!(read_json(&profile_path(home))["groups"][0]["name"], "Two");
    assert!(!names(home).iter().any(|f| f.contains(".tmp")));
}

#[test]
fn a_corrupt_file_is_renamed_profile_corrupt_json_and_bak_loads() {
    let dir = temp_home();
    let home = dir.path();
    let store = ProfileStore::open(home, &starter()).unwrap();
    store.update(|p| p.groups[0].name = "Good".into()).unwrap();
    store.update(|p| p.groups[0].name = "Newer".into()).unwrap();
    write(&profile_path(home), "{ this is not json");
    let again = ProfileStore::open(home, &starter()).unwrap();
    assert_eq!(
        again.load_notice().as_deref(),
        Some("Your profile file was damaged, so Bridge loaded the last good copy.")
    );
    assert_eq!(again.get().groups[0].name, "Good");
    let re = Regex::new(r"^profile\.corrupt-\d{8}-\d{6}\.json$").unwrap();
    let corrupt: Vec<String> = names(home).into_iter().filter(|f| re.is_match(f)).collect();
    assert_eq!(corrupt.len(), 1);
    assert_eq!(read_text(&home.join(&corrupt[0])), "{ this is not json");
    // The good copy is written back as profile.json.
    assert_eq!(read_json(&profile_path(home))["groups"][0]["name"], "Good");
}

#[test]
fn corrupt_file_with_no_usable_bak_falls_back_to_the_starter() {
    let dir = temp_home();
    let home = dir.path();
    ProfileStore::open(home, &starter()).unwrap();
    write(&profile_path(home), "[1,2,3]");
    let again = ProfileStore::open(home, &starter()).unwrap();
    assert_eq!(
        again.load_notice().as_deref(),
        Some("Your profile file was damaged, so Bridge started fresh. The damaged file was kept in the Bridge folder.")
    );
    assert!(again.get().tiles.iter().any(|t| t.name == "Calculator"));
    assert!(names(home).iter().any(|f| f.starts_with("profile.corrupt-")));
}

#[test]
fn on_change_fires_with_the_new_profile_and_can_unsubscribe() {
    let dir = temp_home();
    let home = dir.path();
    let store = ProfileStore::open(home, &starter()).unwrap();
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    let off = store.on_change(Box::new(move |p| sink.lock().unwrap().push(p.groups[0].name.clone())));
    store.update(|p| p.groups[0].name = "A".into()).unwrap();
    store.off(off);
    store.update(|p| p.groups[0].name = "B".into()).unwrap();
    assert_eq!(*seen.lock().unwrap(), vec!["A".to_string()]);
}

#[test]
fn a_throwing_update_changes_nothing() {
    let dir = temp_home();
    let home = dir.path();
    let store = ProfileStore::open(home, &starter()).unwrap();
    let before = read_text(&profile_path(home));
    let err = store
        .try_update(|p| {
            p.groups[0].name = "Nope".into();
            Err("boom".into())
        })
        .unwrap_err();
    assert!(err.contains("boom"), "{err}");
    assert_ne!(store.get().groups[0].name, "Nope");
    assert_eq!(read_text(&profile_path(home)), before);
}

#[test]
fn a_panicking_update_changes_nothing_and_the_store_still_works() {
    let dir = temp_home();
    let home = dir.path();
    let store = ProfileStore::open(home, &starter()).unwrap();
    let before = read_text(&profile_path(home));
    let r = catch_unwind(AssertUnwindSafe(|| {
        store.update(|p| {
            p.groups[0].name = "Nope".into();
            panic!("boom");
        })
    }));
    assert!(r.is_err());
    assert_ne!(store.get().groups[0].name, "Nope");
    assert_eq!(read_text(&profile_path(home)), before);
    store.update(|p| p.groups[0].name = "After".into()).unwrap();
    assert_eq!(store.get().groups[0].name, "After");
}

#[test]
fn get_returns_a_copy_that_cannot_mutate_the_store() {
    let dir = temp_home();
    let home = dir.path();
    let store = ProfileStore::open(home, &starter()).unwrap();
    store.get().groups[0].name = "Mutated".into();
    assert_ne!(store.get().groups[0].name, "Mutated");
}

#[test]
fn profile_store_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ProfileStore>();
}

// ---------------------------------------------------------------- newId

#[test]
fn new_id_has_the_prefix_and_8_base36_chars() {
    assert!(Regex::new(r"^t-[a-z0-9]{8}$").unwrap().is_match(&new_id("t")));
    assert!(Regex::new(r"^g-[a-z0-9]{8}$").unwrap().is_match(&new_id("g")));
    assert!(Regex::new(r"^r-[a-z0-9]{8}$").unwrap().is_match(&new_id("r")));
    assert_ne!(new_id("t"), new_id("t"));
}

// ---------------------------------------------------------------- parseProfile

#[test]
fn parse_profile_fills_settings_defaults_and_drops_unknown_fields() {
    let p = parse_profile(json!({
        "version": 1, "groups": [{ "id": "g1", "name": "A", "junk": 1 }], "tiles": [], "routines": [], "extra": true
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(&p.settings).unwrap(),
        serde_json::to_value(bridge_core::Settings::default()).unwrap()
    );
    let v = serde_json::to_value(&p).unwrap();
    assert!(v.get("extra").is_none());
    assert!(v["groups"][0].get("junk").is_none());
}

#[test]
fn parse_profile_normalizes_start_routine_a_group_orphan_tiles_duplicates_bad_tiles_bad_steps() {
    let p = parse_profile(json!({
        "version": 1,
        "groups": [],
        "tiles": [
            { "id": "t1", "groupId": "nope", "name": "A", "kind": "url", "target": "https://a.com" },
            { "id": "t1", "groupId": "nope", "name": "Dup", "kind": "url", "target": "https://b.com" },
            { "id": "t2", "groupId": "nope", "name": "Bad", "kind": "weird", "target": "x" },
            { "id": "t3", "groupId": "nope", "name": "NoTarget", "kind": "url" },
        ],
        "routines": [{ "id": "r-x", "name": "Other", "steps": [{ "tileId": "t1" }, { "tileId": "gone" }] }],
        "settings": { "hotkey": "Ctrl+Space" },
    }))
    .unwrap();
    assert_eq!(serde_json::to_value(&p.groups).unwrap(), json!([{ "id": "g-work", "name": "Work" }]));
    assert_eq!(p.tiles.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["A"]);
    assert_eq!(p.tiles[0].group_id, "g-work");
    assert_eq!(p.tiles[0].open_count, 0);
    assert!(!p.tiles[0].created_at.is_empty());
    assert_eq!(p.routines.iter().find(|r| r.id == START_MY_DAY_ID).map(|r| r.name.as_str()), Some("Start my day"));
    assert_eq!(
        serde_json::to_value(&p.routines.iter().find(|r| r.id == "r-x").unwrap().steps).unwrap(),
        json!([{ "tileId": "t1" }])
    );
    assert_eq!(p.settings.hotkey, "Ctrl+Space");
    assert_eq!(p.settings.mode, DisplayMode::Auto);
}

#[test]
fn parse_profile_throws_for_non_object_input() {
    assert!(parse_profile(Value::Null).is_err());
    assert!(parse_profile(json!([1, 2])).is_err());
    assert!(parse_profile(json!("x")).is_err());
    assert!(parse_profile(json!({ "version": 1, "tiles": "nope" })).is_err());
}

// ---------------------------------------------------------------- parseProfile edge cases

#[test]
fn an_empty_object_becomes_a_usable_profile() {
    let p = parse_profile(json!({})).unwrap();
    assert_eq!(p.groups.len(), 1);
    assert_eq!(p.routines[0].id, START_MY_DAY_ID);
    assert_eq!(p.settings.hotkey, "Alt+Space");
}

#[test]
fn a_missing_starter_still_opens() {
    let dir = temp_home();
    let home = dir.path();
    let store = ProfileStore::open(home, &home.join("no-such-starter.json")).unwrap();
    assert_eq!(store.get().groups.len(), 1);
}

// ---------------------------------------------------------------- ProfileStore protects hand edits

fn good(extra: Vec<Value>) -> Value {
    let mut tiles = vec![json!({ "id": "t1", "groupId": "g1", "name": "A", "kind": "url", "target": "https://a.example/" })];
    tiles.extend(extra);
    json!({ "version": 1, "groups": [{ "id": "g1", "name": "Work" }], "tiles": tiles, "routines": [] })
}

#[test]
fn keeps_the_original_and_says_so_when_an_item_cannot_be_read() {
    let dir = temp_home();
    let home = dir.path();
    write(
        &profile_path(home),
        &good(vec![json!({ "id": "t2", "groupId": "g1", "name": "B", "kind": "link", "target": "x" })]).to_string(),
    );
    let store = ProfileStore::open(home, &starter()).unwrap();
    assert!(store.load_notice().unwrap().contains("couldn't be read"));
    assert_eq!(copies(home, "dropped").len(), 1);
    assert!(read_text(&home.join(&copies(home, "dropped")[0])).contains("\"t2\""));
    store.update(|p| p.tiles[0].open_count = 1).unwrap();
    store.update(|p| p.tiles[0].open_count = 2).unwrap();
    assert!(read_text(&home.join(&copies(home, "dropped")[0])).contains("\"t2\""));
}

#[test]
fn adopts_an_outside_edit_instead_of_overwriting_it() {
    let dir = temp_home();
    let home = dir.path();
    write(&profile_path(home), &good(vec![]).to_string());
    let store = ProfileStore::open(home, &starter()).unwrap();
    let mut edited = good(vec![]);
    edited["tiles"][0]["name"] = json!("Renamed in Notepad");
    // JSON.stringify(edited, null, 1): a different layout, like a hand edit.
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, serde_json::ser::PrettyFormatter::with_indent(b" "));
    serde::Serialize::serialize(&edited, &mut ser).unwrap();
    write(&profile_path(home), &String::from_utf8(buf).unwrap());
    store.update(|p| p.tiles[0].open_count = 5).unwrap();
    let on_disk = read_json(&profile_path(home));
    assert_eq!(on_disk["tiles"][0]["name"], "Renamed in Notepad");
    assert_eq!(on_disk["tiles"][0]["openCount"], 5);
}

#[test]
fn a_broken_outside_edit_is_kept_aside_and_never_becomes_the_backup() {
    let dir = temp_home();
    let home = dir.path();
    write(&profile_path(home), &good(vec![]).to_string());
    let store = ProfileStore::open(home, &starter()).unwrap();
    store.update(|p| p.tiles[0].open_count = 1).unwrap(); // creates a good .bak
    let notices = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = notices.clone();
    store.on_notice(Box::new(move |m| sink.lock().unwrap().push(m.to_string())));
    write(&profile_path(home), "{ broken");
    store.update(|p| p.tiles[0].open_count = 2).unwrap();
    assert!(notices.lock().unwrap()[0].contains("couldn't be read"));
    assert_eq!(copies(home, "edited").len(), 1);
    assert_eq!(read_text(&home.join(&copies(home, "edited")[0])), "{ broken");
    assert!(serde_json::from_str::<Value>(&read_text(&home.join("profile.json.bak"))).is_ok());
    assert_eq!(read_json(&profile_path(home))["tiles"][0]["openCount"], 2);
}

// ---------------------------------------------------------------- extra: exact TS behaviors

#[test]
fn notices_are_verbatim() {
    use bridge_core::profile::{NOTICE_DROPPED, NOTICE_EXTERNAL_DAMAGED, NOTICE_FROM_BACKUP, NOTICE_FROM_STARTER};
    assert_eq!(NOTICE_FROM_BACKUP, "Your profile file was damaged, so Bridge loaded the last good copy.");
    assert_eq!(
        NOTICE_DROPPED,
        "Some items in your profile couldn't be read, so Bridge left them out. The original file was kept in the Bridge folder."
    );
    assert_eq!(
        NOTICE_EXTERNAL_DAMAGED,
        "Your edited profile file couldn't be read, so Bridge kept its own copy. Your edited file was kept in the Bridge folder."
    );
    assert_eq!(
        NOTICE_FROM_STARTER,
        "Your profile file was damaged, so Bridge started fresh. The damaged file was kept in the Bridge folder."
    );
}

#[test]
fn the_file_is_pretty_json_with_two_spaces_and_a_trailing_newline() {
    let dir = temp_home();
    let home = dir.path();
    let store = ProfileStore::open(home, &starter()).unwrap();
    let text = read_text(&profile_path(home));
    assert!(text.starts_with("{\n  \"version\": 1,\n  \"groups\": [\n    {\n"), "{text}");
    assert!(text.ends_with("}\n") && !text.ends_with("\n\n"));
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), serde_json::to_value(store.get()).unwrap());
}

#[test]
fn a_bom_is_stripped_when_loading() {
    let dir = temp_home();
    let home = dir.path();
    write(&profile_path(home), &format!("\u{FEFF}{}", good(vec![])));
    let store = ProfileStore::open(home, &starter()).unwrap();
    assert_eq!(store.load_notice(), None);
    assert_eq!(store.get().tiles[0].name, "A");
}

#[test]
fn a_broken_file_never_becomes_the_backup_on_open() {
    // Corrupt with no .bak: the corrupt file is moved aside, the starter is written, and no .bak appears.
    let dir = temp_home();
    let home = dir.path();
    write(&profile_path(home), "{ nope");
    let store = ProfileStore::open(home, &starter()).unwrap();
    assert!(store.load_notice().unwrap().contains("started fresh"));
    assert!(!home.join("profile.json.bak").exists());
    assert_eq!(copies(home, "corrupt").len(), 1);
}

#[test]
fn a_second_copy_in_the_same_second_gets_a_counter() {
    let dir = temp_home();
    let home = dir.path();
    write(&profile_path(home), &good(vec![]).to_string());
    let store = ProfileStore::open(home, &starter()).unwrap();
    write(&profile_path(home), "{ broken 1");
    store.update(|p| p.tiles[0].open_count = 1).unwrap();
    write(&profile_path(home), "{ broken 22");
    store.update(|p| p.tiles[0].open_count = 2).unwrap();
    let re = Regex::new(r"^profile\.edited-\d{8}-\d{6}(-\d+)?\.json$").unwrap();
    let edited = copies(home, "edited");
    assert_eq!(edited.len(), 2, "{edited:?}");
    assert!(edited.iter().all(|f| re.is_match(f)), "{edited:?}");
}

#[test]
fn an_outside_edit_with_unreadable_items_is_adopted_and_kept_as_dropped() {
    let dir = temp_home();
    let home = dir.path();
    write(&profile_path(home), &good(vec![]).to_string());
    let store = ProfileStore::open(home, &starter()).unwrap();
    let notices = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = notices.clone();
    store.on_notice(Box::new(move |m| sink.lock().unwrap().push(m.to_string())));
    let edited = good(vec![json!({ "id": "t9", "kind": "nope", "target": "x" }), json!({ "id": "t3", "kind": "url", "target": "https://c.example/" })]);
    write(&profile_path(home), &serde_json::to_string_pretty(&edited).unwrap());
    let out = store.update(|p| p.tiles[0].open_count = 3).unwrap();
    assert_eq!(out.tiles.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["t1", "t3"]);
    assert_eq!(copies(home, "dropped").len(), 1);
    assert!(notices.lock().unwrap()[0].starts_with("Some items in your profile couldn't be read"));
}

#[test]
fn parse_profile_report_counts_dropped_items_but_not_duplicates() {
    let (p, dropped) = parse_profile_report(json!({
        "groups": [{ "id": "g1" }, { "id": "g1", "name": "dup" }, 5, null],
        "tiles": [
            { "id": "t1", "kind": "url", "target": "https://a.com" },
            { "id": "t1", "kind": "url", "target": "https://b.com" },
            "bad",
            { "kind": "app" },
        ],
        "routines": [[], { "id": "r-start" }, { "id": "r-start" }],
    }))
    .unwrap();
    assert_eq!(dropped, 5);
    assert_eq!(p.groups.len(), 1);
    assert_eq!(p.groups[0].name, "Group");
    assert_eq!(p.tiles.len(), 1);
    assert_eq!(p.routines.len(), 1);
    assert_eq!(p.routines[0].name, "Start my day");
    assert!(p.routines[0].steps.is_empty());
}

#[test]
fn parse_profile_trims_and_fills_tile_fields_like_zod() {
    let p = parse_profile(json!({
        "groups": [{ "id": "  g1  ", "name": "  Work  " }],
        "tiles": [
            {
                "id": " t1 ", "groupId": "g1", "name": "   ", "kind": "file", "target": "  C:\\x.txt  ",
                "args": [], "keyword": "  kw  ", "subtitle": " sub ", "icon": "   ", "createdAt": " 2026 ",
                "lastOpenedAt": "", "openCount": 2.0
            },
            { "id": "t2", "groupId": " g1 ", "kind": "url", "target": "https://a", "args": ["a", 1], "openCount": -1 },
            { "id": "t3", "kind": "url", "target": "https://b", "args": ["--x"], "openCount": 1.5 },
        ],
        "routines": [{ "id": "r1", "steps": "nope" }, { "id": "r2", "name": " Mine ", "steps": [{ "tileId": "" }, 3, { "tileId": "t1" }, { "tileId": "t1" }] }],
        "settings": { "hotkey": "   ", "mode": "pinned", "pinnedDisplayId": 12345, "calmMotion": "yes" },
    }))
    .unwrap();
    assert_eq!(p.version, 1);
    assert_eq!(p.groups[0].id, "g1");
    assert_eq!(p.groups[0].name, "Work");
    let t1 = &p.tiles[0];
    assert_eq!(t1.id, "t1");
    assert_eq!(t1.group_id, "g1");
    assert_eq!(t1.target, "C:\\x.txt");
    assert_eq!(t1.name, "C:\\x.txt");
    assert_eq!(t1.args, None);
    assert_eq!(t1.keyword.as_deref(), Some("kw"));
    assert_eq!(t1.subtitle.as_deref(), Some(" sub "));
    assert_eq!(t1.icon, None);
    assert_eq!(t1.created_at, "2026");
    assert_eq!(t1.last_opened_at, None);
    assert_eq!(t1.open_count, 2);
    let t2 = &p.tiles[1];
    assert_eq!(t2.group_id, "g1"); // " g1 " is not a known id, so the first group is used
    assert_eq!(t2.args, None);
    assert_eq!(t2.open_count, 0);
    let t3 = &p.tiles[2];
    assert_eq!(t3.args, Some(vec!["--x".to_string()]));
    assert_eq!(t3.open_count, 0);
    assert_eq!(p.routines[0].id, START_MY_DAY_ID); // unshifted to the front
    assert_eq!(p.routines[1].name, "Routine");
    assert!(p.routines[1].steps.is_empty());
    assert_eq!(p.routines[2].name, "Mine");
    assert_eq!(serde_json::to_value(&p.routines[2].steps).unwrap(), json!([{ "tileId": "t1" }, { "tileId": "t1" }]));
    assert_eq!(p.settings.hotkey, "Alt+Space");
    assert_eq!(p.settings.mode, DisplayMode::Pinned);
    assert_eq!(p.settings.pinned_display_id, Some(12345));
    assert!(!p.settings.calm_motion);
}

#[test]
fn parse_profile_settings_that_are_not_an_object_become_defaults() {
    for s in [json!(null), json!("x"), json!([1]), json!(3)] {
        let p = parse_profile(json!({ "settings": s })).unwrap();
        assert_eq!(serde_json::to_value(&p.settings).unwrap(), serde_json::to_value(bridge_core::Settings::default()).unwrap());
    }
    let p = parse_profile(json!({ "settings": { "hotkey": " Ctrl+K ", "mode": "weird", "pinnedDisplayId": "2", "calmMotion": true } })).unwrap();
    assert_eq!(p.settings.hotkey, "Ctrl+K");
    assert_eq!(p.settings.mode, DisplayMode::Auto);
    assert_eq!(p.settings.pinned_display_id, None);
    assert!(p.settings.calm_motion);
}

#[test]
fn parse_profile_null_lists_are_broken_structure() {
    assert!(parse_profile(json!({ "groups": null })).is_err());
    assert!(parse_profile(json!({ "routines": {} })).is_err());
}
