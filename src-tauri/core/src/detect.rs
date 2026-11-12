//! Works out what a dropped / pasted / typed thing is: a link, an app, a file or a folder. Pure apart from the
//! injected fs probe (and env lookup). Port of `src/main/core/detect.ts`.
use crate::env::{
    expand_env_with, has_env_ref, is_path_like, js_space, js_trim, real_env, win_basename, win_extname, win_normalize,
    win_stem,
};
use crate::schemes::{bare_host_looks_like_web, is_allowed_app_scheme, is_blocked_scheme};
use crate::types::TileKind;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detected {
    pub kind: TileKind,
    pub target: String,
    pub name: String,
    pub subtitle: String,
}

/// What the disk says about a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsKind {
    Dir,
    File,
}

/// File types that start a program, so they become "app" tiles.
const APP_EXTS: &[&str] = &[".exe", ".lnk", ".appref-ms", ".bat", ".cmd", ".url"];

fn scheme_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)^([a-zA-Z][a-zA-Z0-9+.\-]*):(.*)$").unwrap())
}

fn bare_domain_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^((?:[a-zA-Z0-9](?:[a-zA-Z0-9\-]*[a-zA-Z0-9])?\.)+[a-zA-Z]{2,})(:[0-9]{1,5})?([/?#]\S*)?$").unwrap()
    })
}

fn apps_folder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^shell:appsfolder\\.").unwrap())
}

fn host_of(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str()?.to_lowercase();
    let host = host.strip_suffix('.').unwrap_or(&host);
    (!host.is_empty()).then(|| host.to_string())
}

/// True for Domo pages (any host under domo.com). Bridge never fetches metadata for these.
pub fn is_sensitive_url(url: &str) -> bool {
    host_of(url).is_some_and(|h| h.ends_with(".domo.com"))
}

fn url_result(target: String) -> Option<Detected> {
    let host = host_of(&target)?;
    if host.ends_with(".domo.com") {
        return Some(Detected { kind: TileKind::Url, target, name: "Domo page".into(), subtitle: "Domo".into() });
    }
    let name = host.strip_prefix("www.").unwrap_or(&host).to_string();
    Some(Detected { kind: TileKind::Url, target, name, subtitle: host })
}

fn path_result(typed: &str, probe: &dyn Fn(&str) -> Option<FsKind>, env: &dyn Fn(&str) -> Option<String>) -> Option<Detected> {
    let has_env = has_env_ref(typed);
    let absolute = win_normalize(&if has_env { expand_env_with(typed, env) } else { typed.to_string() });
    let found = probe(&absolute)?;
    let target = if has_env { typed.to_string() } else { absolute.clone() };
    let trimmed = absolute.trim_end_matches(['\\', '/']);
    if found == FsKind::Dir {
        let b = trimmed.as_bytes();
        let name = if b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
            trimmed.to_uppercase()
        } else {
            match win_basename(trimmed) {
                "" => trimmed.to_string(),
                n => n.to_string(),
            }
        };
        return Some(Detected { kind: TileKind::Folder, target, name, subtitle: "Folder".into() });
    }
    let ext = win_extname(trimmed).to_lowercase();
    let name = win_stem(trimmed).to_string();
    if APP_EXTS.contains(&ext.as_str()) {
        return Some(Detected { kind: TileKind::App, target, name, subtitle: "App".into() });
    }
    Some(Detected { kind: TileKind::File, target, name, subtitle: win_basename(trimmed).to_string() })
}

/// Detect the kind of an input. Returns `None` for anything Bridge should refuse: plain text, missing paths, and
/// dangerous URIs (javascript:, file:, data:, ...). `probe` gets the absolute path (env vars expanded).
pub fn detect_kind(input: &str, probe: &dyn Fn(&str) -> Option<FsKind>) -> Option<Detected> {
    detect_kind_with(input, probe, &real_env)
}

/// [`detect_kind`] with an injected env lookup (for `%VAR%` paths).
pub fn detect_kind_with(
    input: &str,
    probe: &dyn Fn(&str) -> Option<FsKind>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<Detected> {
    let mut s = js_trim(input);
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s = js_trim(&s[1..s.len() - 1]);
    }
    if s.is_empty() {
        return None;
    }

    // Paths first, so `C:\...` is never read as a URI scheme.
    if is_path_like(s) {
        return path_result(s, probe, env);
    }

    if apps_folder_re().is_match(s) {
        return Some(Detected { kind: TileKind::App, target: s.into(), name: "App".into(), subtitle: "App".into() });
    }

    // Bare domains (example.com/x, www.foo.org) before schemes, so `example.com:8080` isn't a scheme.
    if let Some(bare) = bare_domain_re().captures(s) {
        let has_path_or_port = bare.get(2).is_some() || bare.get(3).is_some();
        return if bare_host_looks_like_web(&bare[1], has_path_or_port) { url_result(format!("https://{s}")) } else { None };
    }

    let m = scheme_re().captures(s)?;
    let scheme = m[1].to_lowercase();
    if is_blocked_scheme(&scheme) {
        return None;
    }
    let has_space = s.chars().any(js_space);
    if scheme == "http" || scheme == "https" {
        if !m[2].starts_with("//") || has_space {
            return None;
        }
        return url_result(s.to_string());
    }
    // Protocol URIs (ms-teams:, mailto:x). Text with spaces like "Note: buy milk" is not a link.
    if has_space || !is_allowed_app_scheme(&scheme) {
        return None;
    }
    Some(Detected { kind: TileKind::App, target: s.into(), subtitle: "Link".into(), name: scheme })
}
