//! Port of tests/unit/detect.test.ts. The fs probe and env lookup are injected closures, so nothing touches the disk
//! or the real environment.
use bridge_core::detect::{detect_kind, detect_kind_with, is_sensitive_url, Detected, FsKind};
use bridge_core::TileKind;

fn dirs(paths: &[&str]) -> impl Fn(&str) -> Option<FsKind> {
    let set: Vec<String> = paths.iter().map(|p| p.to_lowercase()).collect();
    move |p: &str| if set.contains(&p.to_lowercase()) { Some(FsKind::Dir) } else { None }
}
fn files(paths: &[&str]) -> impl Fn(&str) -> Option<FsKind> {
    let set: Vec<String> = paths.iter().map(|p| p.to_lowercase()).collect();
    move |p: &str| if set.contains(&p.to_lowercase()) { Some(FsKind::File) } else { None }
}
fn none(_: &str) -> Option<FsKind> {
    None
}
fn det(kind: TileKind, target: &str, name: &str, subtitle: &str) -> Option<Detected> {
    Some(Detected { kind, target: target.into(), name: name.into(), subtitle: subtitle.into() })
}

// ---- detectKind: urls

#[test]
fn https_url() {
    assert_eq!(
        detect_kind("https://example.com/a", &none),
        det(TileKind::Url, "https://example.com/a", "example.com", "example.com")
    );
}

#[test]
fn bare_domain_with_path_gets_https() {
    let d = detect_kind("example.com/x", &none).unwrap();
    assert_eq!(d.kind, TileKind::Url);
    assert_eq!(d.target, "https://example.com/x");
}

#[test]
fn www_domain() {
    let d = detect_kind("www.foo.org", &none).unwrap();
    assert_eq!(d.kind, TileKind::Url);
    assert_eq!(d.target, "https://www.foo.org");
    assert_eq!(d.name, "foo.org");
    assert_eq!(d.subtitle, "www.foo.org");
}

#[test]
fn domo_urls_are_named_domo_page() {
    let d = detect_kind("https://myco.domo.com/page/123", &none).unwrap();
    assert_eq!((d.kind, d.name.as_str(), d.subtitle.as_str()), (TileKind::Url, "Domo page", "Domo"));
    let d = detect_kind("acme.domo.com/x", &none).unwrap();
    assert_eq!((d.name.as_str(), d.subtitle.as_str()), ("Domo page", "Domo"));
    assert!(is_sensitive_url("https://a.b.domo.com/x"));
    assert!(!is_sensitive_url("https://domo.com.evil.org/"));
    assert!(!is_sensitive_url("not a url"));
}

#[test]
fn trims_whitespace() {
    assert_eq!(detect_kind("  https://example.com  ", &none).unwrap().target, "https://example.com");
}

// ---- detectKind: paths

#[test]
fn folder() {
    assert_eq!(
        detect_kind("C:\\Windows", &dirs(&["C:\\Windows"])),
        det(TileKind::Folder, "C:\\Windows", "Windows", "Folder")
    );
}

#[test]
fn drive_root() {
    let d = detect_kind("D:\\", &dirs(&["D:\\"])).unwrap();
    assert_eq!((d.kind, d.name.as_str()), (TileKind::Folder, "D:"));
}

#[test]
fn file() {
    assert_eq!(
        detect_kind("C:\\x\\notes.txt", &files(&["C:\\x\\notes.txt"])),
        det(TileKind::File, "C:\\x\\notes.txt", "notes", "notes.txt")
    );
}

#[test]
fn exe_is_an_app() {
    assert_eq!(
        detect_kind("C:\\x\\app.exe", &files(&["C:\\x\\app.exe"])),
        det(TileKind::App, "C:\\x\\app.exe", "app", "App")
    );
}

#[test]
fn lnk_is_an_app() {
    let d = detect_kind("C:\\x\\Thing.lnk", &files(&["C:\\x\\Thing.lnk"])).unwrap();
    assert_eq!((d.kind, d.name.as_str()), (TileKind::App, "Thing"));
}

#[test]
fn quoted_path() {
    let d = detect_kind("\"C:\\quoted path\\a.txt\"", &files(&["C:\\quoted path\\a.txt"])).unwrap();
    assert_eq!((d.kind, d.target.as_str(), d.name.as_str()), (TileKind::File, "C:\\quoted path\\a.txt", "a"));
}

#[test]
fn env_var_paths_are_probed_expanded_but_kept_as_typed() {
    let env = |name: &str| (name.eq_ignore_ascii_case("BRIDGE_TEST_DIR")).then(|| "C:\\Users\\someone".to_string());
    let d = detect_kind_with("%BRIDGE_TEST_DIR%\\Documents", &dirs(&["C:\\Users\\someone\\Documents"]), &env);
    assert_eq!(d, det(TileKind::Folder, "%BRIDGE_TEST_DIR%\\Documents", "Documents", "Folder"));
}

#[test]
fn missing_path_is_null() {
    assert_eq!(detect_kind("C:\\nope\\missing.txt", &none), None);
    assert_eq!(detect_kind("C:\\nope", &none), None);
}

#[test]
fn a_drive_path_is_never_read_as_a_scheme() {
    assert_eq!(
        detect_kind("C:/x/notes.txt", &files(&["C:\\x\\notes.txt"])),
        det(TileKind::File, "C:\\x\\notes.txt", "notes", "notes.txt")
    );
    assert_eq!(detect_kind("c:\\x", &none), None);
}

// ---- detectKind: apps and protocols

#[test]
fn shell_appsfolder_is_an_app() {
    let t = "shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";
    assert_eq!(detect_kind(t, &none), det(TileKind::App, t, "App", "App"));
}

#[test]
fn ms_teams_is_an_app() {
    assert_eq!(detect_kind("ms-teams:", &none), det(TileKind::App, "ms-teams:", "ms-teams", "Link"));
}

#[test]
fn mailto_is_an_app() {
    let d = detect_kind("mailto:someone@example.com", &none).unwrap();
    assert_eq!((d.kind, d.name.as_str()), (TileKind::App, "mailto"));
}

// ---- detectKind: junk

#[test]
fn junk_is_null() {
    for input in [
        "javascript:alert(1)",
        "JavaScript:alert(1)",
        "file:///c:/x",
        "data:text/html,x",
        "vbscript:x",
        "about:blank",
        "chrome://settings",
        "edge://settings",
        "hello world",
        "hello",
        "",
        "   ",
        "foo.c0m",
        "example. com",
    ] {
        assert_eq!(detect_kind(input, &none), None, "{input:?} is null");
    }
}

// ---- detectKind: bare names and app links (review fixes)

#[test]
fn bare_file_names_are_not_web_links() {
    for s in ["notes.txt", "report.docx", "invoice.zip", "clip.mov", "Node.js", "hello.world", "readme.md"] {
        assert_eq!(detect_kind(s, &none), None, "{s} is not a web link");
    }
}

#[test]
fn bare_web_names_are_web_links() {
    for s in ["example.com", "team.io", "www.foo.zip", "intranet.local/x", "host.lan:8080"] {
        assert_eq!(detect_kind(s, &none).map(|d| d.kind), Some(TileKind::Url), "{s} is a web link");
    }
}

#[test]
fn allows_known_app_links_and_refuses_unknown_handlers() {
    assert_eq!(detect_kind("ms-teams:", &none).map(|d| d.kind), Some(TileKind::App));
    assert_eq!(detect_kind("mailto:a@b.example", &none).map(|d| d.kind), Some(TileKind::App));
    assert_eq!(detect_kind("ms-officecmd:{\"x\":1}", &none), None);
    assert_eq!(detect_kind("ms-msdt:/id x", &none), None);
    assert_eq!(detect_kind("someapp:run", &none), None);
}

// ---- extra (Rust port): the union blocklist and path-like helpers

#[test]
fn union_blocklist_refuses_search_ms_and_ms_msdt_without_spaces() {
    assert_eq!(detect_kind("search-ms:query=x", &none), None);
    assert_eq!(detect_kind("ms-msdt:/id", &none), None);
}

#[test]
fn is_path_like_and_is_unc() {
    use bridge_core::env::{is_path_like, is_unc};
    assert!(is_path_like("C:\\x"));
    assert!(is_path_like("c:"));
    assert!(is_path_like("\\\\server\\share"));
    assert!(is_path_like("%USERPROFILE%\\x"));
    assert!(!is_path_like("example.com"));
    assert!(!is_path_like("\\rooted"));
    assert!(is_unc("\\\\server\\share"));
    assert!(!is_unc("\\\\\\x"));
    assert!(!is_unc("C:\\x"));
}
