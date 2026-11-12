//! Port of tests/unit/safe-fetch.test.ts. The web is a mock closure; futures run on `block_on`.
use bridge_core::meta_parse::should_fetch_meta;
use bridge_core::safe_fetch::{fetch_checked, FetchResp, DEFAULT_MAX_HOPS};
use futures::executor::block_on;
use futures::future::{ready, Ready};
use std::cell::RefCell;

struct FakeWeb {
    routes: Vec<(&'static str, u16, Option<&'static str>)>,
    requested: RefCell<Vec<String>>,
}

impl FakeWeb {
    fn new(routes: Vec<(&'static str, u16, Option<&'static str>)>) -> Self {
        FakeWeb { routes, requested: RefCell::new(Vec::new()) }
    }
    fn fetch(&self) -> impl FnMut(String) -> Ready<Result<FetchResp, String>> + '_ {
        move |url: String| {
            self.requested.borrow_mut().push(url.clone());
            let (status, location) =
                self.routes.iter().find(|r| r.0 == url).map(|r| (r.1, r.2)).unwrap_or((404, None));
            let body = if (300..400).contains(&status) { Vec::new() } else { b"ok".to_vec() };
            ready(Ok(FetchResp { url, status, location: location.map(String::from), content_type: None, body }))
        }
    }
    fn requested(&self) -> Vec<String> {
        self.requested.borrow().clone()
    }
}

#[test]
fn follows_allowed_redirects_resolving_relative_locations() {
    let w = FakeWeb::new(vec![
        ("https://short.example/x", 302, Some("/landing")),
        ("https://short.example/landing", 200, None),
    ]);
    let r = block_on(fetch_checked(w.fetch(), "https://short.example/x", &should_fetch_meta, DEFAULT_MAX_HOPS)).unwrap();
    assert_eq!(r.url, "https://short.example/landing");
    assert_eq!(r.status, 200);
}

#[test]
fn never_requests_a_redirect_target_on_domo_com() {
    let w = FakeWeb::new(vec![("https://sso.example/go", 302, Some("https://acme.domo.com/page/1"))]);
    assert!(block_on(fetch_checked(w.fetch(), "https://sso.example/go", &should_fetch_meta, DEFAULT_MAX_HOPS)).is_none());
    assert_eq!(w.requested(), ["https://sso.example/go"]);
}

#[test]
fn refuses_a_disallowed_start_url_without_any_request() {
    let w = FakeWeb::new(vec![]);
    assert!(block_on(fetch_checked(w.fetch(), "https://x.domo.com/", &should_fetch_meta, DEFAULT_MAX_HOPS)).is_none());
    assert!(w.requested().is_empty());
}

#[test]
fn gives_up_on_redirect_loops() {
    let w = FakeWeb::new(vec![("https://a.example/", 301, Some("https://a.example/"))]);
    assert!(block_on(fetch_checked(w.fetch(), "https://a.example/", &should_fetch_meta, 3)).is_none());
    assert_eq!(w.requested().len(), 4);
}

// ---- extra (Rust port)

#[test]
fn a_fetch_error_gives_none_and_a_3xx_without_location_is_the_answer() {
    let failing = |_: String| ready(Err::<FetchResp, String>("offline".into()));
    assert!(block_on(fetch_checked(failing, "https://e.example/", &should_fetch_meta, DEFAULT_MAX_HOPS)).is_none());

    let w = FakeWeb::new(vec![("https://e.example/", 304, None)]);
    let r = block_on(fetch_checked(w.fetch(), "https://e.example/", &should_fetch_meta, DEFAULT_MAX_HOPS)).unwrap();
    assert_eq!((r.status, r.url.as_str()), (304, "https://e.example/"));
}
