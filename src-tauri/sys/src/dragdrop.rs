//! Real paths for files dropped from Explorer (replaces Electron's `webUtils.getPathForFile`).
//!
//! The renderer posts `chrome.webview.postMessageWithAdditionalObjects(JSON.stringify({bridgeDrop: id}), files)`;
//! WebView2 hands us the `File`s as `ICoreWebView2File`s, and we answer with the event
//! `bridge:drop-paths` = `{id, paths}`. Every other web message (including Tauri's own IPC) is ignored.
use serde_json::Value;

/// Event name of the reply.
pub const DROP_PATHS_EVENT: &str = "bridge:drop-paths";

/// The `id` of a `{"bridgeDrop": id}` message, or `None` for any other message.
pub fn drop_id(message: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(message).ok()?;
    let id = v.as_object()?.get("bridgeDrop")?;
    id.is_number().then(|| id.clone())
}

/// WebView2 may report either slash style; Windows paths use backslashes.
pub fn normalize_path(p: &str) -> String {
    p.replace('/', "\\")
}

/// Adds the `WebMessageReceived` handler to `window`'s WebView2. (Feature `webview`.)
#[cfg(feature = "webview")]
pub fn install(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    use serde_json::json;
    use tauri::Emitter;
    let win = window.clone();
    window.with_webview(move |wv| {
        use webview2_com::Microsoft::Web::WebView2::Win32::*;
        use windows::core::{Interface, PWSTR};
        // SAFETY: COM calls on the WebView2 UI thread (with_webview runs there); out-params are locals.
        unsafe {
            let core = match wv.controller().CoreWebView2() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[dragdrop] no CoreWebView2: {e}");
                    return;
                }
            };
            let handler = webview2_com::WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut src = PWSTR::null();
                if args.TryGetWebMessageAsString(&mut src).is_err() {
                    return Ok(()); // not a string message: not ours
                }
                let Some(id) = drop_id(&webview2_com::take_pwstr(src)) else { return Ok(()) };
                let mut paths = Vec::new();
                if let Ok(a2) = args.cast::<ICoreWebView2WebMessageReceivedEventArgs2>() {
                    if let Ok(coll) = a2.AdditionalObjects() {
                        let mut n = 0u32;
                        let _ = coll.Count(&mut n);
                        for i in 0..n {
                            let Ok(obj) = coll.GetValueAtIndex(i) else { continue };
                            let Ok(file) = obj.cast::<ICoreWebView2File>() else { continue };
                            let mut p = PWSTR::null();
                            if file.Path(&mut p).is_ok() {
                                let path = webview2_com::take_pwstr(p);
                                if !path.is_empty() {
                                    paths.push(normalize_path(&path));
                                }
                            }
                        }
                    }
                }
                let _ = win.emit(DROP_PATHS_EVENT, json!({ "id": id, "paths": paths }));
                Ok(())
            }));
            let mut token = Default::default();
            if let Err(e) = core.add_WebMessageReceived(&handler, &mut token) {
                eprintln!("[dragdrop] add_WebMessageReceived failed: {e}");
            }
        }
    })
}
