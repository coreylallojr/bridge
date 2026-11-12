//! Title + icon of a web page the user added (port of `fetchUrlMeta` in `src/main/launch/meta.ts`).
//! Pure parsing: `bridge_core::meta_parse`. Redirect vetting: `bridge_core::safe_fetch`.
use std::path::Path;
use std::time::Duration;

use bridge_core::meta_parse::{icon_ext_for, parse_html_meta, should_fetch_meta};
use bridge_core::safe_fetch::{fetch_checked, FetchResp};
use serde::{Deserialize, Serialize};

use crate::win::sha1_hex;

const TIMEOUT: Duration = Duration::from_millis(4000);
const MAX_HTML: usize = 512 * 1024;
const MAX_ICON: usize = 256 * 1024;
const MAX_HOPS: usize = 5;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Bridge/0.3";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// File name inside the icons folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_file: Option<String>,
}

#[derive(Clone, Copy)]
enum Want {
    /// Read up to `MAX_HTML` of an OK html/xml body (truncated beyond that).
    Html,
    /// Read up to `MAX_ICON + 1` of an OK body; more than `MAX_ICON` means "too big".
    Icon,
}

fn client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .ok()
}

fn is_ok(status: u16) -> bool {
    (200..300).contains(&status)
}

fn is_html(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|t| {
        let t = t.to_ascii_lowercase();
        t.contains("html") || t.contains("xml")
    })
}

/// One HTTP hop, without following redirects (a 3xx comes back with its `Location` for `fetch_checked` to vet).
async fn hop(client: reqwest::Client, url: String, want: Want) -> Result<FetchResp, String> {
    let mut req = client.get(&url);
    if let Want::Html = want {
        req = req.header(reqwest::header::ACCEPT, "text/html,*/*;q=0.5");
    }
    let mut res = req.send().await.map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let header = |name: reqwest::header::HeaderName| res.headers().get(name).and_then(|v| v.to_str().ok()).map(String::from);
    let location = header(reqwest::header::LOCATION);
    let content_type = header(reqwest::header::CONTENT_TYPE);
    let cap = match want {
        Want::Html if is_ok(status) && is_html(content_type.as_deref()) => MAX_HTML,
        Want::Icon if is_ok(status) => MAX_ICON + 1,
        _ => 0,
    };
    let mut body = Vec::new();
    while body.len() < cap {
        match res.chunk().await.map_err(|e| e.to_string())? {
            Some(chunk) => body.extend_from_slice(&chunk),
            None => break,
        }
    }
    body.truncate(cap);
    Ok(FetchResp { url, status, location, content_type, body })
}

async fn checked(client: &reqwest::Client, url: &str, want: Want) -> Option<FetchResp> {
    let allow = |u: &str| should_fetch_meta(u);
    let fetch = |u: String| hop(client.clone(), u, want);
    tokio::time::timeout(TIMEOUT, fetch_checked(fetch, url, &allow, MAX_HOPS)).await.ok().flatten()
}

async fn save_icon(client: &reqwest::Client, icon_url: &str, origin: &str, icons_dir: &Path) -> Option<String> {
    let res = checked(client, icon_url, Want::Icon).await?;
    if !is_ok(res.status) {
        return None;
    }
    let ext = icon_ext_for(res.content_type.as_deref())?;
    if res.body.is_empty() || res.body.len() > MAX_ICON {
        return None;
    }
    let file = format!("{}.{}", sha1_hex(origin), ext);
    std::fs::create_dir_all(icons_dir).ok()?;
    std::fs::write(icons_dir.join(&file), &res.body).ok()?;
    Some(file)
}

/// Title and icon (saved as `<icons_dir>\<sha1(origin)>.<ext>`) for a web page. Never errors: any problem gives
/// an empty `UrlMeta`. Never fetches anything `should_fetch_meta` rejects (e.g. domo.com), on any redirect hop.
/// Needs a Tokio runtime (Tauri's async runtime is one).
pub async fn fetch_url_meta(url: &str, icons_dir: &Path) -> UrlMeta {
    if !should_fetch_meta(url) {
        return UrlMeta::default();
    }
    let Some(client) = client() else { return UrlMeta::default() };
    let Some(res) = checked(&client, url, Want::Html).await else { return UrlMeta::default() };
    let mut out = UrlMeta::default();
    let mut icon_href = None;
    if is_ok(res.status) && is_html(res.content_type.as_deref()) {
        let meta = parse_html_meta(&String::from_utf8_lossy(&res.body), &res.url);
        out.title = meta.title.filter(|t| !t.is_empty());
        icon_href = meta.icon_href;
    }
    let Ok(final_url) = url::Url::parse(&res.url) else { return out };
    let origin = final_url.origin().ascii_serialization();
    let favicon = format!("{origin}/favicon.ico");
    let icon_url = match icon_href {
        Some(href) if should_fetch_meta(&href) => href,
        _ => favicon.clone(),
    };
    let mut icon_file = save_icon(&client, &icon_url, &origin, icons_dir).await;
    if icon_file.is_none() && icon_url != favicon {
        icon_file = save_icon(&client, &favicon, &origin, icons_dir).await;
    }
    out.icon_file = icon_file;
    out
}
