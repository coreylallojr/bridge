//! Pure parts of tests/unit/pipe.test.ts: pipe naming, request parsing, framing and the error results.
use bridge_core::command::Command;
use bridge_core::pipe_proto::*;
use bridge_core::Res;

#[test]
fn builds_a_per_user_pipe_name_with_safe_characters() {
    assert_eq!(pipe_short_name("Jane.Doe"), "bridge-jane.doe");
    assert_eq!(pipe_short_name("Pat O Brien$"), "bridge-pat_o_brien_");
    assert_eq!(pipe_short_name("a-b_c.9"), "bridge-a-b_c.9");
}

#[test]
fn non_ascii_is_replaced_per_utf16_unit_like_js() {
    assert_eq!(pipe_short_name("Zoë"), "bridge-zo_");
    // An astral char is two UTF-16 code units in JS, so two underscores.
    assert_eq!(pipe_short_name("a\u{1F600}"), "bridge-a__");
}

#[test]
fn pipe_path_prefixes_the_pipe_namespace() {
    assert_eq!(pipe_path("bridge-jane.doe"), "\\\\.\\pipe\\bridge-jane.doe");
}

#[test]
fn parses_argv_and_cwd() {
    assert_eq!(
        parse_request(r#"{"argv":["open","x"],"cwd":"C:\\work"}"#),
        Ok((vec!["open".to_string(), "x".to_string()], Some("C:\\work".to_string())))
    );
    assert_eq!(parse_request(r#"{"argv":[]}"#), Ok((vec![], None)));
    // Empty or non-string cwd is ignored.
    assert_eq!(parse_request(r#"{"argv":["add","a"],"cwd":""}"#), Ok((vec!["add".into(), "a".into()], None)));
    assert_eq!(parse_request(r#"{"argv":["add","a"],"cwd":5}"#), Ok((vec!["add".into(), "a".into()], None)));
}

#[test]
fn strips_a_bom_and_surrounding_whitespace() {
    assert_eq!(parse_request("\u{FEFF}{\"argv\":[\"start\"]}\r"), Ok((vec!["start".into()], None)));
}

#[test]
fn malformed_requests_are_rejected() {
    assert_eq!(parse_request("this is not json"), Err(()));
    assert_eq!(parse_request(r#"{"argv":[1,2]}"#), Err(()));
    assert_eq!(parse_request(r#"{"argv":"start"}"#), Err(()));
    assert_eq!(parse_request(r#"{}"#), Err(()));
    assert_eq!(parse_request("null"), Err(()));
    assert_eq!(parse_request("[]"), Err(()));
    assert_eq!(parse_request(""), Err(()));
    assert_eq!(parse_request(r#"{"argv":"#), Err(()));
}

#[test]
fn request_command_attaches_cwd_to_add_only() {
    assert_eq!(
        request_command(r#"{"argv":["add","a b"],"cwd":"C:\\w"}"#),
        Some(Command::Add { input: "a b".into(), cwd: Some("C:\\w".into()) })
    );
    assert_eq!(request_command(r#"{"argv":["open","calc","board"],"cwd":"C:\\w"}"#), Some(Command::Open { query: "calc board".into() }));
    assert_eq!(request_command(r#"{"argv":[]}"#), Some(Command::Show));
    assert_eq!(request_command("nope"), None);
}

#[test]
fn error_results_are_verbatim() {
    assert_eq!(BAD_REQUEST, "Bridge could not read that command");
    assert_eq!(HANDLER_FAILED, "Bridge hit a problem running that command");
    assert_eq!(MAX_REQUEST, 64 * 1024);
    assert_eq!(reply_line(&Res::fail(BAD_REQUEST)), "{\"ok\":false,\"message\":\"Bridge could not read that command\"}\n");
    assert_eq!(reply_line(&Res::ok("Opened x \u{2713}")), "{\"ok\":true,\"message\":\"Opened x \u{2713}\"}\n");
}

#[test]
fn framing_takes_the_first_line_or_the_whole_buffer_on_end() {
    assert_eq!(next_request("{\"argv\":[]}\nextra", false), Some("{\"argv\":[]}"));
    assert_eq!(next_request("{\"argv\":", false), None);
    // A client that half-closes without a newline still gets an answer.
    assert_eq!(next_request("{\"argv\":[]}", true), Some("{\"argv\":[]}"));
    // Too much without a newline: answer with an empty (bad) request.
    let big = "x".repeat(MAX_REQUEST + 1);
    assert_eq!(next_request(&big, false), Some(""));
    let exact = "x".repeat(MAX_REQUEST);
    assert_eq!(next_request(&exact, false), None);
}
