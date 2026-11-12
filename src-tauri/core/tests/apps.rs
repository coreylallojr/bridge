//! Port of tests/unit/apps.test.ts.
use bridge_core::apps::parse_start_apps;
use bridge_core::AppEntry;

fn app(name: &str, app_id: &str) -> AppEntry {
    AppEntry { name: name.into(), app_id: app_id.into() }
}

#[test]
fn handles_empty_null_output() {
    assert_eq!(parse_start_apps(""), vec![]);
    assert_eq!(parse_start_apps("   "), vec![]);
    assert_eq!(parse_start_apps("null"), vec![]);
    assert_eq!(parse_start_apps("not json"), vec![]);
}

#[test]
fn handles_a_single_object() {
    assert_eq!(
        parse_start_apps(r#"{"Name":"Calculator","AppID":"Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"}"#),
        vec![app("Calculator", "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App")]
    );
}

#[test]
fn handles_an_array_sorts_by_name_case_insensitively_and_dedups_by_app_id() {
    let json = serde_json::json!([
        { "Name": "zeta", "AppID": "z!App" },
        { "Name": "Alpha", "AppID": "a!App" },
        { "Name": "beta", "AppID": "b!App" },
        { "Name": "Alpha again", "AppID": "a!App" },
    ])
    .to_string();
    assert_eq!(parse_start_apps(&json), vec![app("Alpha", "a!App"), app("beta", "b!App"), app("zeta", "z!App")]);
}

#[test]
fn drops_entries_without_a_name_or_id_and_trims_names() {
    let json = serde_json::json!([
        { "Name": "", "AppID": "x" },
        { "Name": "No id" },
        { "Name": "  Notepad  ", "AppID": "notepad!App" },
        null,
    ])
    .to_string();
    assert_eq!(parse_start_apps(&json), vec![app("Notepad", "notepad!App")]);
}

// ---- extra (Rust port): a UTF-8 BOM from PowerShell, and locale-like ordering

#[test]
fn tolerates_a_bom_and_orders_punctuation_digits_letters_like_locale_compare() {
    let json = "\u{feff}[{\"Name\":\"Écran\",\"AppID\":\"e\"},{\"Name\":\"7-Zip\",\"AppID\":\"7\"},{\"Name\":\"dot\",\"AppID\":\"d\"},{\"Name\":\"_tool\",\"AppID\":\"u\"}]";
    let names: Vec<String> = parse_start_apps(json).into_iter().map(|a| a.name).collect();
    assert_eq!(names, ["_tool", "7-Zip", "dot", "Écran"]);
}
