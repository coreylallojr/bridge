//! Port of tests/unit/open.test.ts. Env lookups are injected, so the real environment never matters.
use bridge_core::env::expand_env_with;
use bridge_core::launch_plan::{find_edge, plan_launch, plan_launch_with, LaunchPlan, WEB_ONLY_REASON};
use bridge_core::{Tile, TileKind};
use std::path::{Path, PathBuf};

const EDGE: &str = "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";
const VARS: &[(&str, &str)] = &[
    ("USERPROFILE", "C:\\Users\\demo"),
    ("ProgramFiles(x86)", "C:\\Program Files (x86)"),
    ("ProgramFiles", "C:\\Program Files"),
];

/// Same as open.ts `lookup`: exact name first, then case-insensitive.
fn vars(name: &str) -> Option<String> {
    VARS.iter()
        .find(|(k, _)| *k == name)
        .or_else(|| VARS.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)))
        .map(|(_, v)| v.to_string())
}

fn tile(kind: TileKind, target: &str) -> Tile {
    Tile {
        id: "t-1".into(),
        group_id: "g-1".into(),
        name: "Thing".into(),
        kind,
        target: target.into(),
        args: None,
        keyword: None,
        subtitle: None,
        icon: None,
        created_at: "2026-11-02T00:00:00.000Z".into(),
        last_opened_at: None,
        open_count: 0,
    }
}
fn plan(t: &Tile) -> LaunchPlan {
    plan_launch_with(t, Some(Path::new(EDGE)), &vars)
}
fn plan_no_edge(t: &Tile) -> LaunchPlan {
    plan_launch_with(t, None, &vars)
}
fn spawn(file: &str, args: &[&str], cwd: Option<&str>) -> LaunchPlan {
    LaunchPlan::Spawn {
        file: file.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        cwd: cwd.map(String::from),
    }
}
fn open_path(p: &str) -> LaunchPlan {
    LaunchPlan::OpenPath { path: p.into() }
}
fn open_external(u: &str) -> LaunchPlan {
    LaunchPlan::OpenExternal { url: u.into() }
}
fn is_refuse(p: &LaunchPlan) -> bool {
    matches!(p, LaunchPlan::Refuse { .. })
}

// ---- planLaunch

#[test]
fn opens_http_s_links_in_a_new_edge_window() {
    assert_eq!(
        plan(&tile(TileKind::Url, "https://example.com/a")),
        spawn(EDGE, &["--new-window", "https://example.com/a"], None)
    );
    match plan(&tile(TileKind::Url, "http://example.com")) {
        LaunchPlan::Spawn { file, .. } => assert_eq!(file, EDGE),
        other => panic!("expected spawn, got {other:?}"),
    }
}

#[test]
fn passes_a_url_containing_amp_as_one_single_argument() {
    let url = "https://example.com/search?q=a&b=c&d=%20e";
    assert_eq!(plan(&tile(TileKind::Url, url)), spawn(EDGE, &["--new-window", url], None));
}

#[test]
fn falls_back_to_the_default_browser_when_edge_is_missing() {
    assert_eq!(plan_no_edge(&tile(TileKind::Url, "https://example.com/")), open_external("https://example.com/"));
}

#[test]
fn refuses_url_tiles_that_are_not_http_https() {
    for bad in ["javascript:alert(1)", "file:///c:/x", "data:text/html,x", "ftp://example.com", "ms-teams:"] {
        match plan(&tile(TileKind::Url, bad)) {
            LaunchPlan::Refuse { reason } => {
                assert_eq!(reason, "Bridge only opens web links that start with http or https", "{bad}");
                assert_eq!(reason, WEB_ONLY_REASON);
            }
            other => panic!("{bad}: expected refuse, got {other:?}"),
        }
    }
}

#[test]
fn refuses_blocked_schemes_on_any_tile_kind() {
    for bad in ["javascript:alert(1)", "JavaScript:x", "file:///c:/x", "data:text/html,x", "vbscript:x", "about:blank"] {
        assert!(is_refuse(&plan(&tile(TileKind::App, bad))), "{bad}");
    }
}

#[test]
fn spawns_explorer_exe_for_shell_appsfolder_targets() {
    let target = "shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";
    assert_eq!(plan(&tile(TileKind::App, target)), spawn("explorer.exe", &[target], None));
}

#[test]
fn spawns_exe_files_with_cwd_their_folder_and_passes_args_through() {
    assert_eq!(
        plan(&tile(TileKind::App, "C:\\Tools\\My App\\app.exe")),
        spawn("C:\\Tools\\My App\\app.exe", &[], Some("C:\\Tools\\My App"))
    );
    let mut t = tile(TileKind::App, "C:\\Tools\\app.EXE");
    t.args = Some(vec!["--flag".into(), "a b".into()]);
    assert_eq!(plan(&t), spawn("C:\\Tools\\app.EXE", &["--flag", "a b"], Some("C:\\Tools")));
}

#[test]
fn opens_lnk_other_app_files_folders_and_files_with_open_path() {
    assert_eq!(plan(&tile(TileKind::App, "C:\\x\\Thing.lnk")), open_path("C:\\x\\Thing.lnk"));
    assert_eq!(plan(&tile(TileKind::App, "C:\\x\\run.bat")), open_path("C:\\x\\run.bat"));
    assert_eq!(plan(&tile(TileKind::App, "C:\\x\\tool.appref-ms")), open_path("C:\\x\\tool.appref-ms"));
    assert_eq!(plan(&tile(TileKind::Folder, "C:\\Windows")), open_path("C:\\Windows"));
    assert_eq!(plan(&tile(TileKind::File, "C:\\x\\notes.txt")), open_path("C:\\x\\notes.txt"));
    assert_eq!(plan(&tile(TileKind::Folder, "\\\\server\\share\\dir")), open_path("\\\\server\\share\\dir"));
}

#[test]
fn expands_var_in_path_targets_and_strips_surrounding_quotes() {
    assert_eq!(plan(&tile(TileKind::Folder, "%USERPROFILE%\\Documents")), open_path("C:\\Users\\demo\\Documents"));
    assert_eq!(plan(&tile(TileKind::File, "\"C:\\quoted path\\a.txt\"")), open_path("C:\\quoted path\\a.txt"));
}

#[test]
fn opens_protocol_uris_with_open_external() {
    assert_eq!(plan(&tile(TileKind::App, "ms-teams:")), open_external("ms-teams:"));
    assert_eq!(plan(&tile(TileKind::App, "ms-settings:display")), open_external("ms-settings:display"));
}

#[test]
fn treats_a_drive_letter_as_a_path_not_a_scheme() {
    assert_eq!(plan(&tile(TileKind::Folder, "D:\\Projects")), open_path("D:\\Projects"));
}

#[test]
fn refuses_empty_and_relative_targets() {
    assert!(is_refuse(&plan(&tile(TileKind::File, "   "))));
    assert!(is_refuse(&plan(&tile(TileKind::File, "notes.txt"))));
}

// ---- expandEnv

#[test]
fn expand_env_replaces_known_variables_case_insensitively_and_leaves_unknown_ones() {
    assert_eq!(expand_env_with("%userprofile%\\x", &vars), "C:\\Users\\demo\\x");
    assert_eq!(expand_env_with("%ProgramFiles(x86)%\\Edge", &vars), "C:\\Program Files (x86)\\Edge");
    assert_eq!(expand_env_with("%NOPE%\\x", &vars), "%NOPE%\\x");
    assert_eq!(expand_env_with("100% sure", &vars), "100% sure");
}

// ---- findEdge

#[test]
fn find_edge_prefers_program_files_x86_then_program_files_else_none() {
    let x86 = PathBuf::from("C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe");
    let x64 = PathBuf::from("C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe");
    assert_eq!(find_edge(&vars, &|_: &Path| true), Some(x86));
    let x64c = x64.clone();
    assert_eq!(find_edge(&vars, &move |p: &Path| p == x64c.as_path()), Some(x64));
    assert_eq!(find_edge(&vars, &|_: &Path| false), None);
}

// ---- planLaunch: app link allowlist (review fixes)

#[test]
fn refuses_risky_links_even_from_a_hand_edited_profile() {
    for t in ["ms-officecmd:{\"a\":1}", "search-ms:query=x", "ms-msdt:/id", "edge://settings", "someapp:go"] {
        assert!(is_refuse(&plan_launch(&tile(TileKind::App, t), None)), "refuses {t}");
    }
}

#[test]
fn still_opens_teams_links() {
    assert_eq!(plan_launch(&tile(TileKind::App, "ms-teams:"), None), open_external("ms-teams:"));
}

// ---- extra (Rust port)

#[test]
fn blocked_reason_is_verbatim_and_rooted_paths_are_refused() {
    assert_eq!(
        plan(&tile(TileKind::App, "search-ms:query=x")),
        LaunchPlan::Refuse { reason: "Bridge won't open that kind of link, for safety".into() }
    );
    assert_eq!(
        plan(&tile(TileKind::File, "  ")),
        LaunchPlan::Refuse { reason: "this tile has no target. Edit it to add one".into() }
    );
    assert_eq!(
        plan(&tile(TileKind::File, "\\Windows\\x.txt")),
        LaunchPlan::Refuse {
            reason: "the target isn't a full path. Edit the tile and use the whole path, like C:\\Folder\\file".into()
        }
    );
}

#[test]
fn launch_plan_serializes_like_the_ts_union() {
    let v = serde_json::to_value(spawn("C:\\a.exe", &[], Some("C:\\"))).unwrap();
    assert_eq!(v, serde_json::json!({"how": "spawn", "file": "C:\\a.exe", "args": [], "cwd": "C:\\"}));
    let v = serde_json::to_value(open_path("C:\\x")).unwrap();
    assert_eq!(v, serde_json::json!({"how": "openPath", "path": "C:\\x"}));
    assert_eq!(plan(&tile(TileKind::App, "C:\\app.exe")), spawn("C:\\app.exe", &[], Some("C:\\")));
}
