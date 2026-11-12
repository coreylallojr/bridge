//! Single instance per `BRIDGE_HOME`. The mutex and the pipe both key on `pipe_short` (which is per-home), so a
//! test instance with its own home never talks to the user's running Bridge.
//! - `acquire`: a named mutex `Local\<pipe_short>`; the handle is kept for the life of the process.
//! - `forward`: a second instance hands its argv to the first over the pipe, then exits.
use bridge_core::pipe_proto::pipe_path;
use bridge_core::Res;
use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, Instant};
use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;

/// True if this process is now the Bridge for `pipe_short`; false if another process (or an earlier call in this
/// process) already holds it, or the mutex can't be created.
pub fn acquire(pipe_short: &str) -> bool {
    let name = HSTRING::from(format!("Local\\{pipe_short}"));
    unsafe {
        let Ok(h) = CreateMutexW(None, false, &name) else { return false };
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(h);
            return false;
        }
        // Deliberately never closed: the mutex lives as long as this process.
        let _ = h;
    }
    true
}

/// How long `forward` keeps trying to connect while the first instance is still starting its pipe server.
const CONNECT_FOR: Duration = Duration::from_secs(2);
/// Same patience as the PowerShell `bridge` function.
const ANSWER_WITHIN: Duration = Duration::from_secs(30);

/// The request line a client sends: `{"argv":[…],"cwd":"…"}\n`.
pub(crate) fn request_line(argv: &[String], cwd: &str) -> String {
    let mut s = serde_json::json!({ "argv": argv, "cwd": cwd }).to_string();
    s.push('\n');
    s
}

/// Sends `argv` + `cwd` to the running Bridge and returns its answer.
pub fn forward(pipe_short: &str, argv: &[String], cwd: &str) -> Result<Res<()>, String> {
    let path = pipe_path(pipe_short);
    let started = Instant::now();
    let mut pipe = loop {
        match std::fs::OpenOptions::new().read(true).write(true).open(&path) {
            Ok(f) => break f,
            // Not created yet, or every instance busy with another client: try again shortly.
            Err(_) if started.elapsed() < CONNECT_FOR => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("Bridge is running but did not answer ({e})")),
        }
    };
    let line = request_line(argv, cwd);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = (|| -> Result<Res<()>, String> {
            pipe.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
            pipe.flush().map_err(|e| e.to_string())?;
            let mut reply = String::new();
            BufReader::new(pipe).read_line(&mut reply).map_err(|e| e.to_string())?;
            serde_json::from_str::<Res<()>>(reply.trim()).map_err(|_| "Bridge did not answer.".to_string())
        })();
        let _ = tx.send(r);
    });
    rx.recv_timeout(ANSWER_WITHIN).unwrap_or_else(|_| Err("Bridge did not answer.".into()))
}

#[cfg(test)]
mod tests {
    use super::request_line;

    #[test]
    fn the_request_is_one_json_line() {
        let line = request_line(&["add".into(), "C:\\x y\\".into(), "say \"hi\"".into()], "C:\\w");
        assert!(line.ends_with('\n') && !line[..line.len() - 1].contains('\n'));
        let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v, serde_json::json!({ "argv": ["add", "C:\\x y\\", "say \"hi\""], "cwd": "C:\\w" }));
    }
}
