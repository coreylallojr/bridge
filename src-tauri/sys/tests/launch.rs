use bridge_sys::launch::LaunchPlan;
use bridge_core::types::{Tile, TileKind};
use bridge_sys::launch::execute;
use serde_json::{json, Value};

fn tile(id: &str, name: &str, kind: TileKind, target: &str) -> Tile {
    Tile {
        id: id.into(),
        group_id: "g-1".into(),
        name: name.into(),
        kind,
        target: target.into(),
        args: None,
        keyword: None,
        subtitle: None,
        icon: None,
        created_at: "2026-11-03T00:00:00.000Z".into(),
        last_opened_at: None,
        open_count: 0,
    }
}

fn log_lines(home: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(home.join("launch-log.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn dry_mode_appends_one_json_line_per_launch() {
    let home = tempfile::tempdir().unwrap();
    let t = tile("t-wiki", "Wiki", TileKind::Url, "https://example.com/a?b=1&c=2");
    let plan = LaunchPlan::Spawn {
        file: r"C:\Edge\msedge.exe".into(),
        args: vec!["--new-window".into(), "https://example.com/a?b=1&c=2".into()],
        cwd: None,
    };
    let r = execute(plan, &t, home.path(), true);
    assert!(r.ok);
    assert_eq!(r.message, "Opened Wiki");

    let r2 = execute(LaunchPlan::OpenPath { path: r"C:\Nowhere\x.txt".into() }, &t, home.path(), true);
    assert!(r2.ok, "dry mode never touches the disk target");

    let lines = log_lines(home.path());
    assert_eq!(lines.len(), 2);
    let l = &lines[0];
    assert_eq!(l["tileId"], "t-wiki");
    assert_eq!(l["name"], "Wiki");
    assert_eq!(
        l["plan"],
        json!({"how": "spawn", "file": r"C:\Edge\msedge.exe", "args": ["--new-window", "https://example.com/a?b=1&c=2"]})
    );
    let at = l["at"].as_str().unwrap();
    assert!(
        regex_like_iso(at),
        "at must look like Date.toISOString(): {at}"
    );
    assert_eq!(lines[1]["plan"], json!({"how": "openPath", "path": r"C:\Nowhere\x.txt"}));
}

fn regex_like_iso(s: &str) -> bool {
    // 2026-11-03T12:34:56.789Z
    s.len() == 24 && s.ends_with('Z') && &s[4..5] == "-" && &s[10..11] == "T" && &s[19..20] == "."
}

#[test]
fn dry_mode_logs_cwd_and_external_urls_in_ts_shape() {
    let home = tempfile::tempdir().unwrap();
    let t = tile("t-calc", "Calculator", TileKind::App, r"C:\Windows\System32\calc.exe");
    let plan = LaunchPlan::Spawn {
        file: r"C:\Windows\System32\calc.exe".into(),
        args: vec![],
        cwd: Some(r"C:\Windows\System32".into()),
    };
    assert!(execute(plan, &t, home.path(), true).ok);
    assert!(execute(LaunchPlan::OpenExternal { url: "ms-teams:".into() }, &t, home.path(), true).ok);
    let lines = log_lines(home.path());
    assert_eq!(
        lines[0]["plan"],
        json!({"how": "spawn", "file": r"C:\Windows\System32\calc.exe", "args": [], "cwd": r"C:\Windows\System32"})
    );
    assert_eq!(lines[1]["plan"], json!({"how": "openExternal", "url": "ms-teams:"}));
}

#[test]
fn refuse_fails_with_the_reason_and_logs_nothing() {
    let home = tempfile::tempdir().unwrap();
    let t = tile("t-x", "Bad", TileKind::Url, "javascript:alert(1)");
    let r = execute(
        LaunchPlan::Refuse { reason: "Bridge only opens web links that start with http or https".into() },
        &t,
        home.path(),
        true,
    );
    assert!(!r.ok);
    assert_eq!(r.message, "Couldn't open Bad: Bridge only opens web links that start with http or https.");
    assert!(!home.path().join("launch-log.jsonl").exists());
}

#[test]
fn a_missing_exe_tile_says_the_program_is_gone() {
    let home = tempfile::tempdir().unwrap();
    let t = tile("t-gone", "Gone", TileKind::App, r"C:\Bridge-test-nowhere\gone.exe");
    let plan = LaunchPlan::Spawn {
        file: r"C:\Bridge-test-nowhere\gone.exe".into(),
        args: vec![],
        cwd: Some(r"C:\Bridge-test-nowhere".into()),
    };
    let r = execute(plan, &t, home.path(), false);
    assert!(!r.ok);
    assert_eq!(r.message, "Couldn't open Gone: the program isn't there anymore.");
}

#[test]
fn a_program_windows_cannot_find_says_so() {
    let home = tempfile::tempdir().unwrap();
    let t = tile("t-nf", "Nope", TileKind::App, "bridge-test-no-such-program.exe");
    let plan = LaunchPlan::Spawn { file: "bridge-test-no-such-program.exe".into(), args: vec![], cwd: None };
    let r = execute(plan, &t, home.path(), false);
    assert!(!r.ok);
    assert_eq!(r.message, "Couldn't open Nope: Windows couldn't find the program.");
}

#[test]
fn missing_files_folders_and_shortcuts_have_their_own_reasons() {
    let home = tempfile::tempdir().unwrap();
    let cases = [
        (TileKind::File, r"C:\Bridge-test-nowhere\a.txt", "Couldn't open X: the file doesn't exist anymore."),
        (TileKind::Folder, r"C:\Bridge-test-nowhere\dir", "Couldn't open X: the folder doesn't exist anymore."),
        (TileKind::App, r"C:\Bridge-test-nowhere\x.lnk", "Couldn't open X: the shortcut isn't there anymore."),
    ];
    for (kind, path, want) in cases {
        let t = tile("t", "X", kind, path);
        let r = execute(LaunchPlan::OpenPath { path: path.into() }, &t, home.path(), false);
        assert!(!r.ok);
        assert_eq!(r.message, want);
    }
}
