//! The named-pipe protocol behind the `bridge` PowerShell function (pure parts of `src/main/cli/pipe.ts`).
//! One JSON line in (`{"argv":[…],"cwd":"…"}`), one JSON `Res` line out.
use serde_json::Value;

use crate::command::{parse_command, Command};
use crate::types::Res;

/// A request longer than this without a newline is answered with [`BAD_REQUEST`].
pub const MAX_REQUEST: usize = 64 * 1024;
/// `Res` message for a line that is not a valid request.
pub const BAD_REQUEST: &str = "Bridge could not read that command";
/// `Res` message when the handler fails (panics / errors).
pub const HANDLER_FAILED: &str = "Bridge hit a problem running that command";

/// `bridge-<username>`: the user part is lowercased and limited to `[a-z0-9._-]` (one `_` per UTF-16 unit, as in TS).
pub fn pipe_short_name(user: &str) -> String {
    let mut out = String::from("bridge-");
    for c in user.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-') {
            out.push(c);
        } else {
            for _ in 0..c.len_utf16() {
                out.push('_');
            }
        }
    }
    out
}

/// `\\.\pipe\<short>`.
pub fn pipe_path(short: &str) -> String {
    format!("\\\\.\\pipe\\{short}")
}

/// Parses one request line (a leading BOM and surrounding whitespace are ignored).
/// Returns `(argv, cwd)`; `cwd` is `None` unless it is a non-empty string.
pub fn parse_request(line: &str) -> Result<(Vec<String>, Option<String>), ()> {
    let line = line.strip_prefix('\u{FEFF}').unwrap_or(line).trim();
    let req: Value = serde_json::from_str(line).map_err(|_| ())?;
    let argv = req.get("argv").and_then(Value::as_array).ok_or(())?;
    let argv = argv.iter().map(|a| a.as_str().map(str::to_string).ok_or(())).collect::<Result<Vec<_>, _>>()?;
    let cwd = req.get("cwd").and_then(Value::as_str).filter(|c| !c.is_empty()).map(str::to_string);
    Ok((argv, cwd))
}

/// `parse_request` + `parse_command`, with `cwd` attached to `add` (TS `parseRequest`). `None` = answer [`BAD_REQUEST`].
pub fn request_command(line: &str) -> Option<Command> {
    let (argv, cwd) = parse_request(line).ok()?;
    Some(match parse_command(&argv) {
        Command::Add { input, .. } => Command::Add { input, cwd },
        c => c,
    })
}

/// Framing: the request line once it is complete, else `None` (keep reading).
/// - a newline ends the request (text after it is ignored);
/// - more than [`MAX_REQUEST`] bytes without one gives `""` (a bad request);
/// - a client that half-closes (`ended`) without a newline still gets its buffer parsed.
pub fn next_request(buf: &str, ended: bool) -> Option<&str> {
    if let Some(nl) = buf.find('\n') {
        Some(&buf[..nl])
    } else if buf.len() > MAX_REQUEST {
        Some("")
    } else if ended {
        Some(buf)
    } else {
        None
    }
}

/// The reply written back: compact JSON + `\n` (use `Res::plain` to drop data first).
pub fn reply_line(res: &Res<()>) -> String {
    let mut s = serde_json::to_string(res).unwrap_or_else(|_| format!("{{\"ok\":false,\"message\":\"{HANDLER_FAILED}\"}}"));
    s.push('\n');
    s
}
