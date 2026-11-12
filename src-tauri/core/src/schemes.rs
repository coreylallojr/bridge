//! One rule, used by both detection (adding) and launching (opening), for app links like ms-teams: or mailto:.
//! An allowlist on purpose: unknown Windows protocol handlers (ms-officecmd:, search-ms:, ms-msdt:…) have been used
//! to run code from a single click, so Bridge only opens link types that are known to be harmless.
//! Port of `src/main/core/schemes.ts`.

pub const ALLOWED_APP_SCHEMES: &[&str] = &[
    "ms-teams", "msteams", "mailto", "tel", "callto", "sip", "im",
    "ms-settings", "ms-screenclip", "ms-clock", "ms-photos", "ms-todo", "calculator",
    "ms-word", "ms-excel", "ms-powerpoint", "onenote", "ms-outlook", "outlook", "ms-whiteboard",
    "vscode", "zoommtg", "zoomus", "slack", "spotify", "webex", "ms-windows-store",
];

/// Schemes that can run script, read local files or open browser internals. Never accepted, never opened.
/// The union of the two TS lists (`detect.ts` and `open.ts`).
pub const BLOCKED_SCHEMES: &[&str] = &[
    "javascript", "file", "data", "vbscript", "about", "chrome", "edge",
    // Known-abused Windows protocol handlers.
    "ms-msdt", "search-ms",
];

/// True if a non-web link of this scheme may be added and opened as an app tile.
pub fn is_allowed_app_scheme(scheme: &str) -> bool {
    let s = scheme.to_lowercase();
    ALLOWED_APP_SCHEMES.contains(&s.as_str())
}

/// True for schemes Bridge refuses outright (case-insensitive).
pub fn is_blocked_scheme(scheme: &str) -> bool {
    let s = scheme.to_lowercase();
    BLOCKED_SCHEMES.contains(&s.as_str())
}

/// Last part of a bare host that is really a file type, not a website ("notes.txt", "invoice.zip", "Node.js").
/// `.zip`, `.mov` and `.md` are real top-level domains, which is exactly why bare names need care.
const FILE_LIKE_TLDS: &[&str] = &[
    "txt", "md", "doc", "docx", "xls", "xlsx", "xlsm", "ppt", "pptx", "pdf", "csv", "json", "xml", "yaml", "yml",
    "zip", "7z", "rar", "mov", "mp4", "mp3", "wav", "png", "jpg", "jpeg", "gif", "svg", "webp",
    "js", "mjs", "ts", "tsx", "py", "ps1", "psm1", "bat", "cmd", "exe", "lnk", "msi", "dll", "ini", "log", "sql",
    "html", "htm", "css", "onnx", "ipynb",
];

/// Common website endings accepted without `www.` or a path (example.com, team.io).
const COMMON_TLDS: &[&str] = &[
    "com", "org", "net", "edu", "gov", "mil", "int", "io", "co", "us", "uk", "ca", "au", "de", "fr", "nl", "eu",
    "dev", "app", "ai", "me", "info", "biz", "tv", "gg", "so", "site", "online", "tech", "cloud", "page",
];

/// Should a scheme-less host like `example.com/x` be read as a web link?
/// Yes for `www.` names, common endings, or a host with a path/port — unless the ending is a file type.
pub fn bare_host_looks_like_web(host: &str, has_path_or_port: bool) -> bool {
    let h = host.to_lowercase();
    let tld = match h.rfind('.') {
        Some(i) => &h[i + 1..],
        None => h.as_str(),
    };
    if h.starts_with("www.") {
        return true;
    }
    if FILE_LIKE_TLDS.contains(&tld) {
        return false;
    }
    COMMON_TLDS.contains(&tld) || has_path_or_port
}
