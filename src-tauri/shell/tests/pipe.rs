//! Port of tests/unit/pipe.test.ts (server side) plus the single-instance checks.
//! Drives the REAL generated `bridge` PowerShell function against `pipe_server::start` with a stub handler.
//! Every test uses its own random pipe name (`bridge-test-…`), so nothing here can reach the user's running Bridge.
#![cfg(windows)]

use bridge_core::command::Command;
use bridge_core::functions::{render_functions_ps1, write_functions_file};
use bridge_core::pipe_proto::{pipe_path, BAD_REQUEST, HANDLER_FAILED, MAX_REQUEST};
use bridge_core::Res;
use bridge_shell::instance::{acquire, forward};
use bridge_shell::pipe_server::{start, PipeHandler, PipeServer};
use futures::FutureExt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

fn unique(tag: &str) -> String {
    static N: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos();
    let name = format!("bridge-test-{tag}-{}-{nanos:x}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst));
    assert_ne!(name, "bridge-jane.doe");
    name
}

type Seen = Arc<Mutex<Vec<Command>>>;

/// The stub from pipe.test.ts: `open` succeeds with `Opened <query> ✓`, everything else answers `Nope`.
/// `run boom` panics, to check the "hit a problem" answer.
fn stub(prefix: &'static str) -> (PipeHandler, Seen) {
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    let handler: PipeHandler = Arc::new(move |c: Command| {
        s.lock().unwrap().push(c.clone());
        async move {
            match c {
                Command::Open { query } => Res::ok(format!("{prefix}Opened {query} \u{2713}")),
                Command::Run { query } if query == "boom" => panic!("handler blew up"),
                _ => Res::fail("Nope"),
            }
        }
        .boxed()
    });
    (handler, seen)
}

fn serve(tag: &str) -> (String, PipeServer, Seen) {
    let name = unique(tag);
    let (h, seen) = stub("");
    let server = start(&name, h).expect("pipe server starts");
    (name, server, seen)
}

async fn connect(name: &str) -> NamedPipeClient {
    let path = pipe_path(name);
    let started = Instant::now();
    loop {
        match ClientOptions::new().open(&path) {
            Ok(c) => return c,
            Err(_) if started.elapsed() < Duration::from_secs(5) => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(e) => panic!("connect {path}: {e}"),
        }
    }
}

/// Send raw bytes and read everything until the server closes.
async fn raw_request(name: &str, bytes: &[u8]) -> String {
    let mut c = connect(name).await;
    // The server may answer (and close) before an oversized request is fully written; the reply still arrives.
    let _ = c.write_all(bytes).await;
    let mut out = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), c.read_to_end(&mut out)).await.expect("server answers").unwrap();
    String::from_utf8(out).unwrap()
}

fn parse(reply: &str) -> serde_json::Value {
    assert!(reply.ends_with('\n') && reply.matches('\n').count() == 1, "one line: {reply:?}");
    serde_json::from_str(reply.trim()).unwrap()
}

async fn powershell(script: String) -> String {
    tokio::task::spawn_blocking(move || {
        let out = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
            .output()
            .expect("powershell.exe");
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    })
    .await
    .unwrap()
}

/// Writes the generated functions file for `pipe` into a fresh temp home (with an apostrophe and a space in it).
fn functions_file(pipe: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::Builder::new().prefix("bridge pipe O'test-").tempdir().unwrap();
    let file = write_functions_file(&dir.path().join("home"), &render_functions_ps1("C:\\nope\\Bridge.exe", pipe)).unwrap();
    let lit = file.to_string_lossy().replace('\'', "''");
    (dir, lit)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bridge_open_calc_board_reaches_the_handler_and_prints_its_message() {
    let (name, server, seen) = serve("ps-open");
    let (_dir, file) = functions_file(&name);
    let out = powershell(format!("$ProgressPreference='SilentlyContinue'; . '{file}'; bridge open calc board 6>&1")).await;
    assert_eq!(seen.lock().unwrap().last(), Some(&Command::Open { query: "calc board".into() }), "{out}");
    assert!(out.contains("Opened calc board"), "{out}");
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_single_arg_and_no_args_still_arrive_as_arrays() {
    let (name, server, seen) = serve("ps-arrays");
    let (_dir, file) = functions_file(&name);
    let out = powershell(format!("$ProgressPreference='SilentlyContinue'; . '{file}'; bridge start 6>&1; bridge 6>&1")).await;
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen, vec![Command::Start, Command::Show], "{out}");
    assert_eq!(out.matches("Nope").count(), 2, "{out}");
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bridge_add_sends_the_folder_it_was_typed_in() {
    let (name, server, seen) = serve("ps-add");
    let (dir, file) = functions_file(&name);
    let cwd = dir.path().to_string_lossy().replace('\'', "''");
    let out = powershell(format!("Set-Location -LiteralPath '{cwd}'; . '{file}'; bridge add 'notes.txt' 6>&1")).await;
    let seen = seen.lock().unwrap().clone();
    // The temp path may be spelled in 8.3 form either way, so compare the folder name.
    let leaf = dir.path().file_name().unwrap().to_string_lossy().to_lowercase();
    match seen.as_slice() {
        [Command::Add { input, cwd: Some(cwd) }] => {
            assert_eq!(input, "notes.txt");
            assert!(cwd.to_lowercase().ends_with(&leaf), "{cwd} vs {leaf}");
        }
        other => panic!("{other:?}
{out}"),
    }
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_malformed_line_gets_the_error_result_and_the_server_keeps_working() {
    let (name, server, seen) = serve("malformed");
    let bad = serde_json::json!({ "ok": false, "message": BAD_REQUEST });
    assert_eq!(parse(&raw_request(&name, b"this is not json\n").await), bad);
    assert_eq!(parse(&raw_request(&name, b"{\"argv\":[1,2]}\n").await)["ok"], false);
    assert_eq!(parse(&raw_request(&name, b"\n").await), bad);
    let good = parse(&raw_request(&name, b"{\"argv\":[\"open\",\"x\"]}\n").await);
    assert_eq!(good, serde_json::json!({ "ok": true, "message": "Opened x \u{2713}" }));
    // A BOM (Windows PowerShell's UTF-8) and CRLF are fine; anything after the first line is ignored.
    let bom = parse(&raw_request(&name, "\u{feff}{\"argv\":[\"open\",\"y\"]}\r\n{\"argv\":[\"start\"]}\n".as_bytes()).await);
    assert_eq!(bom["message"], "Opened y \u{2713}");
    assert_eq!(seen.lock().unwrap().len(), 2, "bad requests never reach the handler");
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_that_hangs_up_early_does_not_crash_the_server() {
    let (name, server, seen) = serve("hangup");
    {
        let mut c = connect(&name).await;
        c.write_all(b"{\"argv\":").await.unwrap();
    } // dropped: hung up mid-request
    {
        let _c = connect(&name).await; // connects and leaves without a word
    }
    let good = parse(&raw_request(&name, b"{\"argv\":[\"open\",\"y\"]}\n").await);
    assert_eq!(good["ok"], true);
    assert_eq!(seen.lock().unwrap().clone(), vec![Command::Open { query: "y".into() }]);
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_without_a_newline_is_still_handled_when_the_client_hangs_up() {
    let (name, server, seen) = serve("no-newline");
    {
        let mut c = connect(&name).await;
        c.write_all(b"{\"argv\":[\"open\",\"z\"]}").await.unwrap();
    }
    let started = Instant::now();
    while seen.lock().unwrap().is_empty() && started.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(seen.lock().unwrap().clone(), vec![Command::Open { query: "z".into() }]);
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn more_than_64_kb_without_a_newline_is_a_bad_request() {
    let (name, server, seen) = serve("cap");
    let big = vec![b'x'; MAX_REQUEST + 4464];
    let reply = parse(&raw_request(&name, &big).await);
    assert_eq!(reply, serde_json::json!({ "ok": false, "message": BAD_REQUEST }));
    // Just under the cap with a newline is read in full (and is still not JSON).
    let mut under = vec![b' '; MAX_REQUEST - 32];
    under.splice(0..0, b"{\"argv\":[\"open\",\"big\"]}".iter().copied());
    under.push(b'\n');
    assert_eq!(parse(&raw_request(&name, &under).await)["message"], "Opened big \u{2713}");
    assert_eq!(seen.lock().unwrap().len(), 1);
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_handler_that_panics_gets_the_problem_result() {
    let (name, server, _seen) = serve("panic");
    let reply = parse(&raw_request(&name, b"{\"argv\":[\"run\",\"boom\"]}\n").await);
    assert_eq!(reply, serde_json::json!({ "ok": false, "message": HANDLER_FAILED }));
    assert_eq!(parse(&raw_request(&name, b"{\"argv\":[\"open\",\"after\"]}\n").await)["ok"], true);
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn many_clients_at_once_all_get_answers() {
    let (name, server, seen) = serve("burst");
    let mut tasks = Vec::new();
    for i in 0..12 {
        let name = name.clone();
        tasks.push(tokio::spawn(async move {
            parse(&raw_request(&name, format!("{{\"argv\":[\"open\",\"n{i}\"]}}\n").as_bytes()).await)
        }));
    }
    for (i, t) in tasks.into_iter().enumerate() {
        assert_eq!(t.await.unwrap()["message"], format!("Opened n{i} \u{2713}"));
    }
    assert_eq!(seen.lock().unwrap().len(), 12);
    server.stop();
}

// ---- single instance ------------------------------------------------------------------------------------------

#[test]
fn acquire_twice_on_the_same_name_the_second_is_refused() {
    let name = unique("mutex");
    assert!(acquire(&name), "first instance gets it");
    assert!(!acquire(&name), "second instance is refused");
    assert!(acquire(&unique("mutex-other")), "another home is independent");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_server_on_the_same_pipe_is_refused() {
    let (name, server, _seen) = serve("dupe");
    let (h, _) = stub("");
    assert!(start(&name, h).is_err());
    server.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_homes_two_servers_answer_independently_and_forward_carries_argv_and_cwd() {
    let (a, b) = (unique("home-a"), unique("home-b"));
    let (ha, seen_a) = stub("A: ");
    let (hb, seen_b) = stub("B: ");
    let (sa, sb) = (start(&a, ha).unwrap(), start(&b, hb).unwrap());

    let (a2, b2) = (a.clone(), b.clone());
    let (ra, rb, radd) = tokio::task::spawn_blocking(move || {
        (
            forward(&a2, &["open".into(), "q".into()], "C:\\a"),
            forward(&b2, &["Bridge.exe".into(), "open".into(), "q".into()], "C:\\b"),
            forward(&b2, &["add".into(), "x y.txt".into()], "C:\\work dir"),
        )
    })
    .await
    .unwrap();
    assert_eq!(ra.unwrap(), Res::ok("A: Opened q \u{2713}"));
    assert_eq!(rb.unwrap(), Res::ok("B: Opened q \u{2713}"));
    assert_eq!(radd.unwrap(), Res::fail("Nope"));
    assert_eq!(seen_a.lock().unwrap().clone(), vec![Command::Open { query: "q".into() }]);
    assert_eq!(
        seen_b.lock().unwrap().clone(),
        vec![
            Command::Open { query: "q".into() },
            Command::Add { input: "x y.txt".into(), cwd: Some("C:\\work dir".into()) }
        ]
    );
    sa.stop();
    sb.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn forward_waits_for_a_starting_instance_then_gives_up() {
    let name = unique("late");
    let n2 = name.clone();
    let pending = tokio::task::spawn_blocking(move || forward(&n2, &["start".into()], "C:\\"));
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (h, seen) = stub("");
    let server = start(&name, h).unwrap();
    assert_eq!(pending.await.unwrap().unwrap(), Res::fail("Nope"));
    assert_eq!(seen.lock().unwrap().clone(), vec![Command::Start]);
    server.stop();

    let absent = unique("absent");
    let started = Instant::now();
    let r = tokio::task::spawn_blocking(move || forward(&absent, &[], "C:\\")).await.unwrap();
    assert!(r.is_err());
    assert!(started.elapsed() >= Duration::from_millis(1500), "it retried for ~2 s");
}
