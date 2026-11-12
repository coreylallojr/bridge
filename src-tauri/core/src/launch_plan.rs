//! Pure launch planning: decides HOW a tile opens. The side that executes a plan lives in the app crate.
//! Port of `src/main/launch/open.ts`.
use crate::env::{expand_env_with, js_trim, real_env, win_dirname, win_extname, win_is_absolute};
use crate::schemes::{is_allowed_app_scheme, is_blocked_scheme};
use crate::types::{Tile, TileKind};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "how", rename_all = "camelCase")]
pub enum LaunchPlan {
    Spawn {
        file: String,
        args: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    OpenPath {
        path: String,
    },
    OpenExternal {
        url: String,
    },
    Refuse {
        reason: String,
    },
}

pub const WEB_ONLY_REASON: &str = "Bridge only opens web links that start with http or https";
pub const BLOCKED_REASON: &str = "Bridge won't open that kind of link, for safety";
pub const NO_TARGET_REASON: &str = "this tile has no target. Edit it to add one";
pub const NOT_FULL_PATH_REASON: &str =
    "the target isn't a full path. Edit the tile and use the whole path, like C:\\Folder\\file";

fn refuse(reason: &str) -> LaunchPlan {
    LaunchPlan::Refuse { reason: reason.into() }
}

/// Scheme of a URI-like string ("https", "ms-teams"…), or None. One-letter schemes are drive letters, not schemes.
fn scheme_of(s: &str) -> Option<String> {
    let colon = s.find(':')?;
    let name = &s[..colon];
    let b = name.as_bytes();
    let ok = b.len() >= 2
        && b[0].is_ascii_alphabetic()
        && b[1..].iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'.' | b'-'));
    ok.then(|| name.to_lowercase())
}

/// Microsoft Edge's exe, preferring the usual x86 install location.
pub fn find_edge(get_env: &dyn Fn(&str) -> Option<String>, exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    const TAIL: &str = "\\Microsoft\\Edge\\Application\\msedge.exe";
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        let Some(base) = get_env(var) else { continue };
        if base.is_empty() {
            continue;
        }
        let p = PathBuf::from(format!("{}{TAIL}", base.trim_end_matches(['\\', '/'])));
        if exists(&p) {
            return Some(p);
        }
    }
    None
}

fn unquote(s: &str) -> &str {
    let t = js_trim(s);
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        js_trim(&t[1..t.len() - 1])
    } else {
        t
    }
}

fn plan_web(url: &str, edge: Option<&Path>) -> LaunchPlan {
    match url::Url::parse(url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some_and(|h| !h.is_empty()) => {}
        _ => return refuse(WEB_ONLY_REASON),
    }
    // Each arg is passed separately (no shell), so `&` and spaces in the URL stay intact.
    match edge {
        Some(e) => LaunchPlan::Spawn {
            file: e.to_string_lossy().into_owned(),
            args: vec!["--new-window".into(), url.into()],
            cwd: None,
        },
        None => LaunchPlan::OpenExternal { url: url.into() },
    }
}

/// How to open `tile`, expanding `%VAR%` from the real environment. `edge` = [`find_edge`]'s answer.
pub fn plan_launch(tile: &Tile, edge: Option<&Path>) -> LaunchPlan {
    plan_launch_with(tile, edge, &real_env)
}

/// [`plan_launch`] with an injected env lookup.
pub fn plan_launch_with(tile: &Tile, edge: Option<&Path>, get_env: &dyn Fn(&str) -> Option<String>) -> LaunchPlan {
    let target = unquote(&tile.target);
    if target.is_empty() {
        return refuse(NO_TARGET_REASON);
    }

    let scheme = scheme_of(target);
    let is_web = matches!(scheme.as_deref(), Some("http" | "https"));
    if tile.kind == TileKind::Url {
        return if is_web { plan_web(target, edge) } else { refuse(WEB_ONLY_REASON) };
    }

    if let Some(scheme) = scheme {
        if is_blocked_scheme(&scheme) {
            return refuse(BLOCKED_REASON);
        }
        if is_web {
            return plan_web(target, edge);
        }
        // shell:AppsFolder\<AppID> (Store/Start-menu apps) and other shell: folders open via Explorer.
        if scheme == "shell" {
            return LaunchPlan::Spawn { file: "explorer.exe".into(), args: vec![target.into()], cwd: None };
        }
        // Same allowlist as adding, so a hand-edited profile can't sneak in a risky link type.
        if !is_allowed_app_scheme(&scheme) {
            return refuse(BLOCKED_REASON);
        }
        return LaunchPlan::OpenExternal { url: target.into() };
    }

    let path = expand_env_with(target, get_env);
    let b = path.as_bytes();
    let rooted_not_unc = !b.is_empty() && matches!(b[0], b'\\' | b'/') && !(b.len() > 1 && matches!(b[1], b'\\' | b'/'));
    if !win_is_absolute(&path) || rooted_not_unc {
        return refuse(NOT_FULL_PATH_REASON);
    }
    if win_extname(&path).eq_ignore_ascii_case(".exe") {
        let cwd = win_dirname(&path);
        return LaunchPlan::Spawn { file: path, args: tile.args.clone().unwrap_or_default(), cwd: Some(cwd) };
    }
    // Folders, files, .lnk shortcuts and other launchable files (.bat/.cmd/.url/.appref-ms) use the Windows default action.
    LaunchPlan::OpenPath { path }
}
