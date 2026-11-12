//! Pure CLI parsing for `bridge …` (pipe requests and second-instance argv). Port of `src/main/cli/command.ts`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Show,
    Start,
    Open { query: String },
    Run { query: String },
    /// `cwd` = folder the command was typed in, for relative paths.
    Add { input: String, cwd: Option<String> },
    Help,
    Error { message: String },
}

pub const HELP_TEXT: &str = "bridge                     Show Bridge\n\
bridge start               Run Start my day\n\
bridge open <name>         Open a tile, like: bridge open kanban\n\
bridge run <name>          Run a routine\n\
bridge add <link or path>  Add a link, app, file or folder";

const HELP_WORDS: [&str; 4] = ["help", "-h", "/?", "-?"];

/// Leading argv noise from a launch: the exe path, `.` and empty args.
fn is_launch_noise(arg: &str) -> bool {
    arg.is_empty() || arg == "." || arg.to_ascii_lowercase().ends_with(".exe")
}

pub fn parse_command(argv: &[String]) -> Command {
    // Switches can land anywhere in a second instance's argv; Bridge commands never take `--` options.
    let args: Vec<&str> = argv.iter().map(String::as_str).filter(|a| !a.starts_with("--")).collect();
    let rest = &args[args.iter().take_while(|a| is_launch_noise(a)).count()..];
    let Some((word, rest)) = rest.split_first() else { return Command::Show };
    let tail = rest.join(" ").trim().to_string();
    let lower = word.to_lowercase();
    match lower.as_str() {
        "start" => Command::Start,
        "open" | "run" => {
            if tail.is_empty() {
                Command::Error { message: format!("Tell me what to {lower}, like: bridge {lower} kanban") }
            } else if lower == "open" {
                Command::Open { query: tail }
            } else {
                Command::Run { query: tail }
            }
        }
        "add" => {
            if tail.is_empty() {
                Command::Error { message: "Tell me what to add, like: bridge add https://example.com".into() }
            } else {
                Command::Add { input: tail, cwd: None }
            }
        }
        w if HELP_WORDS.contains(&w) => Command::Help,
        _ => Command::Error {
            message: format!(
                "Unknown command \"{word}\". Try: bridge, bridge start, bridge open <name>, bridge run <name>, bridge add <link or path>"
            ),
        },
    }
}
