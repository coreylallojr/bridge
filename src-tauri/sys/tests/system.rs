use bridge_sys::{clipboard, dragdrop, stats};
use serde_json::json;

#[test]
fn stats_read_raw_gives_sane_counters() {
    let (idle, total, free, mem_total) = stats::read_raw();
    assert!(total > 0 && idle <= total, "idle {idle} total {total}");
    assert!(mem_total > 0 && free <= mem_total, "free {free} total {mem_total}");
    std::thread::sleep(std::time::Duration::from_millis(50));
    let (_, total2, _, _) = stats::read_raw();
    assert!(total2 >= total);
}

#[test]
fn drop_messages_are_recognized_and_everything_else_ignored() {
    assert_eq!(dragdrop::drop_id(r#"{"bridgeDrop":7}"#), Some(json!(7)));
    assert_eq!(dragdrop::drop_id(r#"{"bridgeDrop":"7"}"#), None);
    assert_eq!(dragdrop::drop_id(r#"{"cmd":"get_profile","callback":1}"#), None);
    assert_eq!(dragdrop::drop_id("not json"), None);
    assert_eq!(dragdrop::drop_id("[1,2]"), None);
}

#[test]
fn dropped_paths_use_backslashes() {
    assert_eq!(dragdrop::normalize_path("C:/Users/x/My File.txt"), r"C:\Users\x\My File.txt");
    assert_eq!(dragdrop::normalize_path(r"C:\a\b"), r"C:\a\b");
}

/// Touches the real clipboard, so it only runs on request: `node scripts/rust.mjs test -p bridge-sys -- --ignored`.
/// Saves the user's clipboard text first and puts it back afterwards.
#[test]
#[ignore]
fn clipboard_round_trips_trimmed_text() {
    let Ok(saved) = clipboard_win::get_clipboard_string() else {
        eprintln!("clipboard holds no text (maybe a file or image); skipped so nothing is lost");
        return;
    };
    assert!(clipboard::write_text("  C:\\Bridge test\\x.txt \r\n"));
    let got = clipboard::read_input();
    clipboard::write_text(&saved);
    assert_eq!(got, "C:\\Bridge test\\x.txt");
}
