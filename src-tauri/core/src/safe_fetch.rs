//! Redirects followed by hand, so every hop is checked BEFORE it is requested (e.g. never land on *.domo.com).
//! Pure: the fetch function is injected and must NOT follow redirects itself. Port of `src/main/launch/safe-fetch.ts`.
use std::future::Future;

/// One raw HTTP answer (redirects not followed).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FetchResp {
    /// The URL that was requested for this answer.
    pub url: String,
    pub status: u16,
    /// The `Location` header, if any.
    pub location: Option<String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// `maxHops` default in the TS.
pub const DEFAULT_MAX_HOPS: usize = 5;

/// Requests `url`, following up to `max_hops` redirects. Every URL (the first and each redirect target) must pass
/// `allow` before it is requested. Returns the final answer with `url` set to the URL that produced it; `None` if a
/// hop is not allowed, there are too many redirects, a `Location` can't be resolved, or `fetch` fails.
pub async fn fetch_checked<F, Fut>(
    mut fetch: F,
    url: &str,
    allow: &(dyn Fn(&str) -> bool + Sync),
    max_hops: usize,
) -> Option<FetchResp>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<FetchResp, String>>,
{
    let mut current = url.to_string();
    for _hop in 0..=max_hops {
        if !allow(&current) {
            return None;
        }
        let mut res = fetch(current.clone()).await.ok()?;
        let location = if (300..400).contains(&res.status) { res.location.clone().filter(|l| !l.is_empty()) } else { None };
        let Some(location) = location else {
            res.url = current;
            return Some(res);
        };
        current = url::Url::parse(&current).ok()?.join(&location).ok()?.to_string();
    }
    None // too many redirects
}
