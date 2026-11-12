//! Pure helpers for URL metadata (title + icon). The fetching side lives in the app crate.
//! Port of `src/main/launch/meta-parse.ts`.
use crate::env::js_trim;
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HtmlMeta {
    pub title: Option<String>,
    /// Absolute, http/https only.
    pub icon_href: Option<String>,
}

macro_rules! re {
    ($pat:expr) => {{
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new($pat).unwrap())
    }};
}

fn named(body: &str) -> Option<&'static str> {
    Some(match body.to_lowercase().as_str() {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => " ",
        _ => return None,
    })
}

/// Decodes `&amp;`-style named entities (a small set) and numeric `&#…;` / `&#x…;` references.
pub fn decode_entities(s: &str) -> String {
    re!(r"&(#[xX][0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);")
        .replace_all(s, |c: &regex::Captures| {
            let whole = &c[0];
            let body = &c[1];
            if let Some(num) = body.strip_prefix('#') {
                let code = match num.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => num.parse::<u32>().ok(),
                };
                return match code.filter(|c| *c > 0 && *c <= 0x10ffff).and_then(char::from_u32) {
                    Some(ch) => ch.to_string(),
                    None => whole.to_string(),
                };
            }
            named(body).map(String::from).unwrap_or_else(|| whole.to_string())
        })
        .into_owned()
}

fn clean(s: &str) -> String {
    let decoded = decode_entities(s);
    let collapsed = re!(r"\s+").replace_all(&decoded, " ");
    js_trim(&collapsed).to_string()
}

/// Attributes of one tag's source text, keys lowercased. Handles "x", 'x' and unquoted values.
fn attrs(tag: &str) -> HashMap<String, String> {
    let body = re!(r"^<\s*[a-zA-Z]+").replace(tag, "");
    let body = re!(r"/?>$").replace(&body, "");
    let mut out = HashMap::new();
    let attr = re!(r#"([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*(?:=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?"#);
    for m in attr.captures_iter(&body) {
        let val = m.get(2).or(m.get(3)).or(m.get(4)).map_or("", |v| v.as_str());
        out.insert(m[1].to_lowercase(), val.to_string());
    }
    out
}

fn resolve_http(href: &str, base: &str) -> Option<String> {
    let base = url::Url::parse(base).ok()?;
    let u = base.join(&decode_entities(js_trim(href))).ok()?;
    matches!(u.scheme(), "http" | "https").then(|| u.to_string())
}

/// Title and icon link of an HTML page. `icon_href` is absolute (http/https only).
pub fn parse_html_meta(html: &str, base: &str) -> HtmlMeta {
    let mut out = HtmlMeta::default();

    let title = re!(r"(?is)<title[^>]*>(.*?)</title\s*>").captures(html).map(|c| clean(&c[1])).unwrap_or_default();
    if !title.is_empty() {
        out.title = Some(title);
    } else {
        for m in re!(r"(?i)<meta\b[^>]*>").find_iter(html) {
            let a = attrs(m.as_str());
            let key = a.get("property").or_else(|| a.get("name"));
            if key.is_some_and(|k| k.to_lowercase() == "og:title") {
                if let Some(content) = a.get("content").filter(|c| !c.is_empty()) {
                    let c = clean(content);
                    if !c.is_empty() {
                        out.title = Some(c);
                        break;
                    }
                }
            }
        }
    }

    let mut icon: Option<String> = None;
    let mut touch: Option<String> = None;
    for m in re!(r"(?i)<link\b[^>]*>").find_iter(html) {
        let a = attrs(m.as_str());
        let Some(href) = a.get("href").filter(|h| !h.is_empty()) else { continue };
        let rels = a.get("rel").map(|r| r.to_lowercase()).unwrap_or_default();
        let has = |want: &str| rels.split(crate::env::js_space).any(|r| r == want);
        if icon.is_none() && has("icon") {
            icon = resolve_http(href, base);
        }
        if touch.is_none() && has("apple-touch-icon") {
            touch = resolve_http(href, base);
        }
    }
    out.icon_href = icon.or(touch);
    out
}

/// Only http/https pages, and never anything on domo.com (CLAUDE.md hard rule).
pub fn should_fetch_meta(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else { return false };
    if !matches!(u.scheme(), "http" | "https") {
        return false;
    }
    let host = u.host_str().unwrap_or("").to_lowercase();
    let host = host.strip_suffix('.').unwrap_or(&host);
    host != "domo.com" && !host.ends_with(".domo.com")
}

/// File extension for a saved icon, from its Content-Type; None if it isn't an image we keep.
pub fn icon_ext_for(content_type: Option<&str>) -> Option<&'static str> {
    let t = content_type.unwrap_or("");
    let t = js_trim(t.split(';').next().unwrap_or("")).to_lowercase();
    Some(match t.as_str() {
        "image/png" => "png",
        "image/x-icon" | "image/vnd.microsoft.icon" | "image/ico" | "image/icon" => "ico",
        "image/svg+xml" => "svg",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => return None,
    })
}
