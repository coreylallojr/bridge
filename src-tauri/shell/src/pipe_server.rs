//! Named-pipe server behind the `bridge` PowerShell function. Port of `src/main/cli/pipe.ts` (server side).
//! - One JSON line in (`{argv, cwd}`), one JSON `Res` line out, then the pipe closes.
//! - A client that sends without a trailing newline and hangs up is still handled (its reply can't be delivered).
//! - More than 64 KB without a newline is answered as a bad request.
//! - Only the current user can open the pipe: its DACL is `D:P(A;;GA;;;<user SID>)`; remote clients are refused.
//! Framing and parsing live in core `pipe_proto`; this file is the I/O.
use bridge_core::command::Command;
use bridge_core::pipe_proto::{next_request, pipe_path, reply_line, request_command, BAD_REQUEST, HANDLER_FAILED};
use bridge_core::Res;
use futures::future::BoxFuture;
use futures::FutureExt;
use std::ffi::c_void;
use std::io;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Runs one parsed command and returns the answer for the caller.
pub type PipeHandler = Arc<dyn Fn(Command) -> BoxFuture<'static, Res<()>> + Send + Sync>;

/// A running server. Dropping it does NOT stop it (so `let _ = start(..)` keeps serving); call `stop`.
pub struct PipeServer {
    task: tokio::task::JoinHandle<()>,
}

impl PipeServer {
    /// Stops accepting new clients. Requests already being answered finish.
    pub fn stop(&self) {
        self.task.abort();
    }
}

/// A security descriptor allocated by Windows (`LocalAlloc`), freed on drop.
struct Sd(*mut c_void);
// Read-only after creation; Windows only reads it when creating pipe instances.
unsafe impl Send for Sd {}
unsafe impl Sync for Sd {}
impl Drop for Sd {
    fn drop(&mut self) {
        unsafe { LocalFree(Some(HLOCAL(self.0))) };
    }
}

fn win_err(e: windows::core::Error) -> io::Error {
    io::Error::other(e.message())
}

/// String SID of the user running this process (`S-1-5-21-…`).
fn current_user_sid() -> io::Result<String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(win_err)?;
        let result = (|| {
            let mut len = 0u32;
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len); // sizing call "fails" by design
            let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
            GetTokenInformation(token, TokenUser, Some(buf.as_mut_ptr().cast()), len, &mut len).map_err(win_err)?;
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s = PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut s).map_err(win_err)?;
            let sid = s.to_string().map_err(|e| io::Error::other(e.to_string()));
            LocalFree(Some(HLOCAL(s.0.cast())));
            sid
        })();
        let _ = CloseHandle(token);
        result
    }
}

/// SDDL that grants the pipe to exactly one user and nobody else (protected DACL, no inheritance).
pub(crate) fn user_only_sddl(sid: &str) -> String {
    format!("D:P(A;;GA;;;{sid})")
}

fn current_user_sd() -> io::Result<Sd> {
    let sddl = HSTRING::from(user_only_sddl(&current_user_sid()?));
    let mut sd = PSECURITY_DESCRIPTOR::default();
    unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(&sddl, SDDL_REVISION_1, &mut sd, None) }
        .map_err(win_err)?;
    Ok(Sd(sd.0))
}

fn create(path: &str, first: bool, sd: &Sd) -> io::Result<NamedPipeServer> {
    let mut sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: false.into(),
    };
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(path, &mut sa as *mut SECURITY_ATTRIBUTES as *mut c_void)
    }
}

/// Keeps trying to open the next listening instance (it only fails if Windows is out of resources).
async fn create_retrying(path: &str, sd: &Sd) -> NamedPipeServer {
    loop {
        match create(path, false, sd) {
            Ok(s) => return s,
            Err(e) => {
                eprintln!("[bridge] pipe server error: {e}");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

/// Starts serving `\\.\pipe\<pipe_short>`. Fails if the pipe already exists (another Bridge owns it) or can't be
/// created. Runs on the current tokio runtime if there is one (tests), else on Tauri's async runtime.
pub fn start(pipe_short: &str, handler: PipeHandler) -> io::Result<PipeServer> {
    let rt = tokio::runtime::Handle::try_current().unwrap_or_else(|_| tauri::async_runtime::handle().inner().clone());
    let _enter = rt.enter(); // pipe instances register with the runtime's reactor when created
    let path = pipe_path(pipe_short);
    let sd = current_user_sd()?;
    let first = create(&path, true, &sd)?;
    let task = rt.spawn(async move {
        let mut server = first;
        loop {
            match server.connect().await {
                Ok(()) => {
                    let next = create_retrying(&path, &sd).await;
                    let client = std::mem::replace(&mut server, next);
                    tokio::spawn(serve(client, handler.clone()));
                }
                Err(e) => {
                    // The client vanished between connecting and us noticing; open a fresh instance.
                    eprintln!("[bridge] pipe server error: {e}");
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    server = create_retrying(&path, &sd).await;
                }
            }
        }
    });
    Ok(PipeServer { task })
}

/// Answers one client: read one request line, run it, write one reply line, close.
async fn serve(mut pipe: NamedPipeServer, handler: PipeHandler) {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    let line = loop {
        let n = match pipe.read(&mut chunk).await {
            Ok(n) => n,
            Err(_) => return, // a broken connection: nobody to answer
        };
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf);
        if let Some(l) = next_request(&text, n == 0) {
            break l.to_string();
        }
        if n == 0 {
            return;
        }
    };
    let res = match request_command(&line) {
        None => Res::fail(BAD_REQUEST),
        // The call itself is inside the guarded future too, so a handler that panics before returning is caught.
        Some(cmd) => AssertUnwindSafe(async move { handler(cmd).await })
            .catch_unwind()
            .await
            .unwrap_or_else(|_| Res::fail(HANDLER_FAILED)),
    };
    // The client may already be gone (hung up early); that's fine.
    let _ = pipe.write_all(reply_line(&res).as_bytes()).await;
    let _ = pipe.flush().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dacl_names_only_the_current_user() {
        let sid = current_user_sid().unwrap();
        assert!(sid.starts_with("S-1-5-"), "{sid}");
        assert_eq!(user_only_sddl(&sid), format!("D:P(A;;GA;;;{sid})"));
        assert!(current_user_sd().is_ok());
    }
}
