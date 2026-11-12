//! `%VAR%` expansion and the small `node:path` win32 helpers Bridge relies on (normalize, basename, extname, dirname,
//! isAbsolute). One `expand_env` replaces the two TS expanders (`detect.ts` and `open.ts`).
use regex::Regex;
use std::sync::OnceLock;

/// `%NAME%` like cmd.exe. Names can't contain path or wildcard characters, so `100% sure` and `50%/x%` stay as-is.
fn env_var_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"%([^%\\/:*?"<>|\r\n]+)%"#).unwrap())
}

/// Case-insensitive lookup in the real process environment (Windows env names are case-insensitive).
pub fn real_env(name: &str) -> Option<String> {
    // `var_os` may reject (or panic on) these keys; the scan below handles them safely.
    if !name.is_empty() && !name.contains(['=', '\0']) {
        if let Ok(v) = std::env::var(name) {
            return Some(v);
        }
    }
    let lower = name.to_lowercase();
    std::env::vars_os()
        .find(|(k, _)| k.to_string_lossy().to_lowercase() == lower)
        .and_then(|(_, v)| v.into_string().ok())
}

/// Expands `%VAR%` using `lookup`. Unknown variables are left as typed.
pub fn expand_env_with(s: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    env_var_re()
        .replace_all(s, |c: &regex::Captures| lookup(&c[1]).unwrap_or_else(|| c[0].to_string()))
        .into_owned()
}

/// Expands `%VAR%` from the real environment (case-insensitive). Unknown variables are left as typed.
pub fn expand_env(s: &str) -> String {
    expand_env_with(s, &real_env)
}

/// True if a string contains a `%VAR%`-looking part (`detect.ts` rule).
pub(crate) fn has_env_ref(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"%[^%\\/]+%").unwrap()).is_match(s)
}

/// Starts like a Windows path: `C:` / `C:\…`, a UNC `\\server…`, or a leading `%VAR%` (`api.ts` PATH_LIKE).
pub fn is_path_like(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:[a-zA-Z]:(?:[\\/]|$)|\\\\[^\\]|%[^%\\/]+%)").unwrap()).is_match(s)
}

/// A UNC path (`\\server\share…`): two backslashes then a name.
pub fn is_unc(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 3 && b[0] == b'\\' && b[1] == b'\\' && b[2] != b'\\'
}

/// `String.prototype.trim`: White_Space plus the BOM.
pub(crate) fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// JS `\s`.
pub(crate) fn js_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

// ---- node:path win32 ports ----------------------------------------------------------------------------------------

fn is_sep(b: u8) -> bool {
    b == b'\\' || b == b'/'
}

fn has_drive(b: &[u8]) -> bool {
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// `path.win32.isAbsolute`.
pub fn win_is_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    if b.is_empty() {
        return false;
    }
    is_sep(b[0]) || (b.len() > 2 && has_drive(b) && is_sep(b[2]))
}

/// Length of the root of `p` (`C:\` = 3, `C:` = 2, `\\server\share\` incl. the separator, `\` = 1).
fn root_len(b: &[u8]) -> usize {
    let len = b.len();
    if len == 0 {
        return 0;
    }
    if is_sep(b[0]) {
        if len > 1 && is_sep(b[1]) {
            // UNC: \\server\share
            let mut j = 2;
            let last = j;
            while j < len && !is_sep(b[j]) {
                j += 1;
            }
            if j < len && j != last {
                let last = j;
                while j < len && is_sep(b[j]) {
                    j += 1;
                }
                if j < len && j != last {
                    let last = j;
                    while j < len && !is_sep(b[j]) {
                        j += 1;
                    }
                    if j == len {
                        return j;
                    }
                    if j != last {
                        return j + 1;
                    }
                }
            }
        }
        return 1;
    }
    if has_drive(b) {
        return if len > 2 && is_sep(b[2]) { 3 } else { 2 };
    }
    0
}

/// Resolves `.` / `..` and collapses separators (Node's `normalizeString`).
fn normalize_tail(path: &str, allow_above_root: bool) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split(['\\', '/']) {
        match seg {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else if allow_above_root {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    out.join("\\")
}

/// `path.win32.normalize`.
pub fn win_normalize(path: &str) -> String {
    let b = path.as_bytes();
    let len = b.len();
    if len == 0 {
        return ".".into();
    }
    if len == 1 {
        return if b[0] == b'/' { "\\".into() } else { path.into() };
    }
    let mut root_end = 0;
    let mut device: Option<String> = None;
    let mut is_absolute = false;
    if is_sep(b[0]) {
        is_absolute = true;
        root_end = 1;
        if is_sep(b[1]) {
            root_end = 0;
            let mut j = 2;
            let mut last = j;
            while j < len && !is_sep(b[j]) {
                j += 1;
            }
            if j < len && j != last {
                let first_part = &path[last..j];
                last = j;
                while j < len && is_sep(b[j]) {
                    j += 1;
                }
                if j < len && j != last {
                    last = j;
                    while j < len && !is_sep(b[j]) {
                        j += 1;
                    }
                    if j == len {
                        // A UNC root only.
                        return format!("\\\\{}\\{}\\", first_part, &path[last..]);
                    }
                    if j != last {
                        device = Some(format!("\\\\{}\\{}", first_part, &path[last..j]));
                        root_end = j;
                    }
                }
            }
        }
    } else if has_drive(b) {
        device = Some(path[..2].to_string());
        root_end = 2;
        if len > 2 && is_sep(b[2]) {
            is_absolute = true;
            root_end = 3;
        }
    }
    let mut tail = if root_end < len { normalize_tail(&path[root_end..], !is_absolute) } else { String::new() };
    if tail.is_empty() && !is_absolute {
        tail = ".".into();
    }
    if !tail.is_empty() && is_sep(b[len - 1]) {
        tail.push('\\');
    }
    match device {
        None if is_absolute => format!("\\{tail}"),
        None => tail,
        Some(d) if is_absolute => format!("{d}\\{tail}"),
        Some(d) => format!("{d}{tail}"),
    }
}

/// `path.win32.basename` (trailing separators ignored; a drive prefix is never part of the name).
pub fn win_basename(p: &str) -> &str {
    let start = if has_drive(p.as_bytes()) { 2 } else { 0 };
    let s = p[start..].trim_end_matches(['\\', '/']);
    match s.rfind(['\\', '/']) {
        Some(i) => &s[i + 1..],
        None => s,
    }
}

/// Extension of a base name, Node rules: `.bashrc` and `..` have none, `a.b.TXT` has `.TXT`.
fn ext_of_base(base: &str) -> &str {
    let b = base.as_bytes();
    let mut start_dot: Option<usize> = None;
    let mut pre_dot_state = 0i8;
    for i in (0..b.len()).rev() {
        if b[i] == b'.' {
            if start_dot.is_none() {
                start_dot = Some(i);
            } else if pre_dot_state != 1 {
                pre_dot_state = 1;
            }
        } else if start_dot.is_some() {
            pre_dot_state = -1;
        }
    }
    match start_dot {
        None => "",
        Some(_) if pre_dot_state == 0 => "",
        Some(d) if pre_dot_state == 1 && d == b.len() - 1 && d == 1 => "",
        Some(d) => &base[d..],
    }
}

/// `path.win32.extname`.
pub fn win_extname(p: &str) -> &str {
    ext_of_base(win_basename(p))
}

/// `path.win32.parse(p).name` (base name without its extension).
pub fn win_stem(p: &str) -> &str {
    let base = win_basename(p);
    &base[..base.len() - ext_of_base(base).len()]
}

/// `path.win32.dirname`: the folder part; the root (`C:\`, `\\server\share\`) is kept whole.
pub fn win_dirname(p: &str) -> String {
    let b = p.as_bytes();
    if b.is_empty() {
        return ".".into();
    }
    let root = root_len(b);
    let rest = &p[root..];
    let trimmed = rest.trim_end_matches(['\\', '/']);
    match trimmed.rfind(['\\', '/']) {
        Some(i) => {
            let dir = p[..root + i].trim_end_matches(['\\', '/']);
            // Never trim into the root (`C:\\x` → `C:\`).
            if dir.len() < root { p[..root].to_string() } else { dir.to_string() }
        }
        None if root > 0 => p[..root].to_string(),
        None => ".".into(),
    }
}
