//! Port of tests/unit/match.test.ts, plus the shared vectors in tests/fixtures/match-vectors.json (also run by Vitest).
use bridge_core::matcher::rank;

struct Item {
    name: &'static str,
    keyword: Option<&'static str>,
}
fn it(name: &'static str) -> Item {
    Item { name, keyword: None }
}
fn fields(t: &Item) -> Vec<String> {
    vec![t.name.to_string(), t.keyword.unwrap_or("").to_string()]
}
fn names(xs: Vec<&Item>) -> Vec<&'static str> {
    xs.into_iter().map(|x| x.name).collect()
}

#[test]
fn empty_query_returns_empty() {
    assert!(rank("", &[it("A")], fields).is_empty());
    assert!(rank("   ", &[it("A")], fields).is_empty());
}

#[test]
fn exact_keyword_or_name_case_insensitive_ranks_first() {
    let items = [it("Calc helper"), Item { name: "Calculator", keyword: Some("calc") }, it("CALC")];
    assert_eq!(names(rank("Calc", &items, fields)), ["Calculator", "CALC", "Calc helper"]);
}

#[test]
fn tiers_exact_prefix_substring_all_words_subsequence() {
    let items = [
        it("Kanban board"),    // prefix
        it("board xx kanban"), // substring
        it("my kanban"),       // substring
        it("kanban"),          // exact
        it("kanban stuff"),    // prefix
    ];
    assert_eq!(
        names(rank("kanban", &items, fields)),
        ["kanban", "Kanban board", "kanban stuff", "board xx kanban", "my kanban"]
    );
    assert_eq!(names(rank("board kanban", &items, fields)), ["Kanban board", "board xx kanban"]);
}

#[test]
fn subsequence_kb_matches_kanban_board() {
    assert_eq!(names(rank("kb", &[it("Notepad"), it("Kanban board")], fields)), ["Kanban board"]);
}

#[test]
fn ranks_by_tier_in_order() {
    let items = [
        it("kxaxnxbxo"),   // subsequence
        it("bob kanga"),   // all words
        it("zkan boz"),    // substring
        it("kan bo more"), // prefix
        it("KAN BO"),      // exact
    ];
    assert_eq!(
        names(rank("kan bo", &items, fields)),
        ["KAN BO", "kan bo more", "zkan boz", "bob kanga", "kxaxnxbxo"]
    );
}

#[test]
fn ties_keep_input_order() {
    let items = [it("Note 2"), it("Note 1"), it("Note 3")];
    assert_eq!(names(rank("note", &items, fields)), ["Note 2", "Note 1", "Note 3"]);
}

#[test]
fn returns_nothing_for_non_matches() {
    assert!(rank("zzz", &[it("Notepad")], fields).is_empty());
}

#[test]
fn subsequence_tier_spaces_in_the_query_are_ignored() {
    assert_eq!(names(rank("k bd", &[it("Kanban board")], fields)), ["Kanban board"]);
}

#[test]
fn shared_match_vectors() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/match-vectors.json");
    let raw = std::fs::read_to_string(path).expect("read match-vectors.json");
    let vectors: serde_json::Value = serde_json::from_str(&raw).expect("parse match-vectors.json");
    let vectors = vectors.as_array().expect("array");
    assert!(vectors.len() >= 12, "expected at least 12 vectors");
    for v in vectors {
        let query = v["query"].as_str().unwrap();
        let items: Vec<Vec<String>> = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fs| fs.as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect())
            .collect();
        let expected: Vec<usize> =
            v["expected"].as_array().unwrap().iter().map(|i| i.as_u64().unwrap() as usize).collect();
        let got: Vec<usize> = rank(query, &items, |f| f.clone())
            .into_iter()
            .map(|r| items.iter().position(|x| std::ptr::eq(x, r)).unwrap())
            .collect();
        assert_eq!(got, expected, "query {query:?}");
    }
}
