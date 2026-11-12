//! Clipboard (port of `src/main/system/clipboard.ts` + `clipboard.writeText` in `api.ts`).
use clipboard_win::{formats, get_clipboard, get_clipboard_string, set_clipboard_string};

/// What Ctrl+V should add: the path of the first file copied in Explorer (CF_HDROP), else the trimmed clipboard
/// text, else `""`.
pub fn read_input() -> String {
    if let Ok(files) = get_clipboard::<Vec<String>, _>(formats::FileList) {
        if let Some(first) = files.into_iter().map(|f| f.trim().to_string()).find(|f| !f.is_empty()) {
            return first;
        }
    }
    get_clipboard_string().map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Puts plain text on the clipboard. `false` if Windows wouldn't let us (another app holds it open).
pub fn write_text(text: &str) -> bool {
    set_clipboard_string(text).is_ok()
}
