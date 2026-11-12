use std::thread;

use bridge_sys::meta::{fetch_url_meta, UrlMeta};
use tiny_http::{Header, Response, Server};

const PNG_BYTES: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];
const ICO_BYTES: &[u8] = &[0, 0, 1, 0, 1, 0, 16, 16];

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

/// A tiny local site. Returns its origin (`http://127.0.0.1:<port>`).
fn serve() -> String {
    let server = Server::http("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", server.server_addr().to_ip().unwrap());
    thread::spawn(move || {
        for req in server.incoming_requests() {
            let html = |body: &str| {
                Response::from_string(body.to_string()).with_header(header("Content-Type", "text/html; charset=utf-8"))
            };
            let _ = match req.url() {
                "/page" => req.respond(html(
                    "<html><head><title> Hello &amp; welcome </title><link rel=\"icon\" href=\"/i.png\"></head></html>",
                )),
                "/i.png" => req.respond(Response::from_data(PNG_BYTES).with_header(header("Content-Type", "image/png"))),
                "/redir" => req.respond(Response::empty(302).with_header(header("Location", "/landed"))),
                "/landed" => req.respond(html("<title>Landed</title>")),
                "/data.json" => req.respond(
                    Response::from_string("{\"title\":\"no\"}").with_header(header("Content-Type", "application/json")),
                ),
                "/big" => req.respond(html("<title>Big icon</title><link rel=\"icon\" href=\"/big.png\">")),
                "/big.png" => req.respond(
                    Response::from_data(vec![7u8; 300 * 1024]).with_header(header("Content-Type", "image/png")),
                ),
                "/favicon.ico" => {
                    req.respond(Response::from_data(ICO_BYTES).with_header(header("Content-Type", "image/x-icon")))
                }
                "/to-domo" => req.respond(Response::empty(302).with_header(header("Location", "https://x.domo.com/"))),
                _ => req.respond(Response::empty(404)),
            };
        }
    });
    origin
}

fn sha1_hex(s: &str) -> String {
    use sha1::{Digest, Sha1};
    format!("{:x}", Sha1::digest(s.as_bytes()))
}

#[tokio::test]
async fn title_and_linked_icon_are_saved() {
    let origin = serve();
    let icons = tempfile::tempdir().unwrap();
    let meta = fetch_url_meta(&format!("{origin}/page"), icons.path()).await;
    assert_eq!(meta.title.as_deref(), Some("Hello & welcome"));
    let file = meta.icon_file.expect("icon saved");
    assert_eq!(file, format!("{}.png", sha1_hex(&origin)));
    assert_eq!(std::fs::read(icons.path().join(&file)).unwrap(), PNG_BYTES);
}

#[tokio::test]
async fn a_redirect_is_followed_to_the_landed_page_title() {
    let origin = serve();
    let icons = tempfile::tempdir().unwrap();
    let meta = fetch_url_meta(&format!("{origin}/redir"), icons.path()).await;
    assert_eq!(meta.title.as_deref(), Some("Landed"));
    // No icon link on the page: falls back to /favicon.ico.
    assert_eq!(meta.icon_file, Some(format!("{}.ico", sha1_hex(&origin))));
}

#[tokio::test]
async fn a_non_html_response_has_no_title_but_still_gets_the_favicon() {
    let origin = serve();
    let icons = tempfile::tempdir().unwrap();
    let meta = fetch_url_meta(&format!("{origin}/data.json"), icons.path()).await;
    assert_eq!(meta.title, None);
    let file = meta.icon_file.expect("favicon saved");
    assert_eq!(std::fs::read(icons.path().join(&file)).unwrap(), ICO_BYTES);
}

#[tokio::test]
async fn an_icon_over_256_kb_is_rejected_and_the_favicon_used_instead() {
    let origin = serve();
    let icons = tempfile::tempdir().unwrap();
    let meta = fetch_url_meta(&format!("{origin}/big"), icons.path()).await;
    assert_eq!(meta.title.as_deref(), Some("Big icon"));
    assert_eq!(meta.icon_file, Some(format!("{}.ico", sha1_hex(&origin))));
}

#[tokio::test]
async fn a_redirect_to_domo_and_unreachable_hosts_give_empty_meta() {
    let origin = serve();
    let icons = tempfile::tempdir().unwrap();
    assert_eq!(fetch_url_meta(&format!("{origin}/to-domo"), icons.path()).await, UrlMeta::default());
    assert_eq!(fetch_url_meta("https://domo.com/", icons.path()).await, UrlMeta::default());
    assert_eq!(fetch_url_meta("ftp://example.com/", icons.path()).await, UrlMeta::default());
    // Port 9 on localhost: nothing listens, connection refused.
    assert_eq!(fetch_url_meta("http://127.0.0.1:9/", icons.path()).await, UrlMeta::default());
    assert!(std::fs::read_dir(icons.path()).unwrap().next().is_none(), "nothing saved");
}
