//! Start-menu app list parsing (`Get-StartApps`). Running PowerShell lives in the sys crate.
//! Port of `parseStartApps` in `src/main/launch/apps.ts`.
use crate::env::js_trim;
use crate::types::AppEntry;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashSet;

/// Parses `Get-StartApps | Select-Object Name,AppID | ConvertTo-Json` output (one object, an array, or nothing).
/// Entries without a name or id are dropped, names are trimmed, duplicates (by AppID) keep the first, and the list
/// is sorted by name ignoring case and accents.
pub fn parse_start_apps(json: &str) -> Vec<AppEntry> {
    let text = js_trim(json);
    if text.is_empty() {
        return Vec::new();
    }
    let Ok(raw) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let list = match raw {
        Value::Array(a) => a,
        Value::Null | Value::Bool(false) => Vec::new(),
        Value::String(ref s) if s.is_empty() => Vec::new(),
        Value::Number(ref n) if n.as_f64() == Some(0.0) => Vec::new(),
        other => vec![other],
    };
    let mut seen = HashSet::new();
    let mut out: Vec<AppEntry> = Vec::new();
    for item in &list {
        let Value::Object(o) = item else { continue };
        let field = |k: &str| o.get(k).and_then(Value::as_str).map(|s| js_trim(s).to_string()).unwrap_or_default();
        let (name, app_id) = (field("Name"), field("AppID"));
        if name.is_empty() || app_id.is_empty() || !seen.insert(app_id.clone()) {
            continue;
        }
        out.push(AppEntry { name, app_id });
    }
    out.sort_by(|a, b| locale_compare_base(&a.name, &b.name)); // stable, like Array.prototype.sort
    out
}

/// Approximates `a.localeCompare(b, undefined, { sensitivity: 'base' })` (ICU root collation, primary strength):
/// whitespace < punctuation < symbols < digits < letters; case and common Latin accents are ignored.
fn locale_compare_base(a: &str, b: &str) -> Ordering {
    let key = |s: &str| s.chars().map(|c| (class_of(c), fold(c))).collect::<Vec<_>>();
    key(a).cmp(&key(b))
}

fn class_of(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_ascii_punctuation() {
        // ASCII symbols (Unicode S*) sort after punctuation (P*).
        if "$+<=>^`|~".contains(c) { 2 } else { 1 }
    } else if c.is_numeric() {
        3
    } else if c.is_alphabetic() {
        4
    } else {
        2
    }
}

fn fold(c: char) -> char {
    const TABLE: &[(&str, char)] = &[
        ("àáâãäåāăą", 'a'),
        ("çćĉċč", 'c'),
        ("ďđ", 'd'),
        ("èéêëēĕėęě", 'e'),
        ("ĝğġģ", 'g'),
        ("ĥħ", 'h'),
        ("ìíîïĩīĭįı", 'i'),
        ("ĵ", 'j'),
        ("ķ", 'k'),
        ("ĺļľŀł", 'l'),
        ("ñńņňŉ", 'n'),
        ("òóôõöøōŏő", 'o'),
        ("ŕŗř", 'r'),
        ("śŝşšſ", 's'),
        ("ţťŧ", 't'),
        ("ùúûüũūŭůűų", 'u'),
        ("ŵ", 'w'),
        ("ýÿŷ", 'y'),
        ("źżž", 'z'),
    ];
    let lower = c.to_lowercase().next().unwrap_or(c);
    if lower.is_ascii() {
        return lower;
    }
    TABLE.iter().find(|(set, _)| set.contains(lower)).map_or(lower, |(_, base)| *base)
}
