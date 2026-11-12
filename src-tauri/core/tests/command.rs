//! Port of tests/unit/command.test.ts. Noise stripping is the Tauri rule: drop `--*`, then leading "", "." and `*.exe`.
use bridge_core::command::{parse_command, Command, HELP_TEXT};

fn argv(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn parse(a: &[&str]) -> Command {
    parse_command(&argv(a))
}

#[test]
fn no_args_shows_bridge() {
    assert_eq!(parse(&[]), Command::Show);
}

#[test]
fn start() {
    assert_eq!(parse(&["start"]), Command::Start);
    assert_eq!(parse(&["START"]), Command::Start);
}

#[test]
fn open_joins_the_words_into_one_query() {
    assert_eq!(parse(&["open", "kanban", "board"]), Command::Open { query: "kanban board".into() });
    assert_eq!(parse(&["run", "focus"]), Command::Run { query: "focus".into() });
}

#[test]
fn open_run_without_a_name_is_a_friendly_error() {
    assert_eq!(
        parse(&["open"]),
        Command::Error { message: "Tell me what to open, like: bridge open kanban".into() }
    );
    assert_eq!(
        parse(&["run", "  "]),
        Command::Error { message: "Tell me what to run, like: bridge run kanban".into() }
    );
    // The word is lowercased in the message, as in TS.
    assert_eq!(
        parse(&["OPEN"]),
        Command::Error { message: "Tell me what to open, like: bridge open kanban".into() }
    );
}

#[test]
fn add_keeps_a_path_with_spaces() {
    assert_eq!(parse(&["add", "C:\\x y\\a.txt"]), Command::Add { input: "C:\\x y\\a.txt".into(), cwd: None });
    assert_eq!(
        parse(&["add"]),
        Command::Error { message: "Tell me what to add, like: bridge add https://example.com".into() }
    );
}

#[test]
fn help() {
    assert_eq!(parse(&["help"]), Command::Help);
    assert_eq!(parse(&["-h"]), Command::Help);
    assert_eq!(parse(&["/?"]), Command::Help);
    assert_eq!(parse(&["-?"]), Command::Help);
    assert_eq!(parse(&["HELP"]), Command::Help);
    assert!(HELP_TEXT.contains("bridge open"));
}

#[test]
fn help_text_is_verbatim() {
    assert_eq!(
        HELP_TEXT,
        "bridge                     Show Bridge\n\
         bridge start               Run Start my day\n\
         bridge open <name>         Open a tile, like: bridge open kanban\n\
         bridge run <name>          Run a routine\n\
         bridge add <link or path>  Add a link, app, file or folder"
    );
}

#[test]
fn unknown_words_are_an_error_that_lists_the_commands() {
    assert_eq!(
        parse(&["dance"]),
        Command::Error {
            message: "Unknown command \"dance\". Try: bridge, bridge start, bridge open <name>, bridge run <name>, bridge add <link or path>".into()
        }
    );
}

#[test]
fn strips_the_exe_and_switches() {
    let exe = "C:\\Users\\me\\AppData\\Local\\Bridge\\Bridge.exe";
    assert_eq!(parse(&[exe, "."]), Command::Show);
    assert_eq!(parse(&[exe]), Command::Show);
    assert_eq!(parse(&[exe, "--allow-file-access-from-files", "start"]), Command::Start);
    assert_eq!(
        parse(&[exe, "--inspect=9229", "open", "kanban", "--original-process-start-time=1"]),
        Command::Open { query: "kanban".into() }
    );
    assert_eq!(
        parse(&["C:\\Program Files\\Bridge\\Bridge.exe", "open", "calc"]),
        Command::Open { query: "calc".into() }
    );
    assert_eq!(parse(&["BRIDGE.EXE", "", ".", "start"]), Command::Start);
}

#[test]
fn only_leading_noise_is_skipped() {
    // A later "." or ".exe" is part of the command, not noise.
    assert_eq!(parse(&["add", "C:\\tools\\calc.exe"]), Command::Add { input: "C:\\tools\\calc.exe".into(), cwd: None });
    assert_eq!(parse(&["open", "."]), Command::Open { query: ".".into() });
    // Single-dash switches are no longer noise (Tauri never adds them).
    assert!(matches!(parse(&["-x", "start"]), Command::Error { .. }));
}
