//! Fuzzy ranking for the command palette and `bridge open <word>`. Port of `src/shared/match.ts`; the shared vectors
//! in `tests/fixtures/match-vectors.json` keep the two in step.

fn norm(s: &str) -> String {
    crate::env::js_trim(&s.to_lowercase()).to_string()
}

fn is_subsequence(needle: &[char], hay: &str) -> bool {
    let mut i = 0;
    for c in hay.chars() {
        if i == needle.len() {
            break;
        }
        if c == needle[i] {
            i += 1;
        }
    }
    i == needle.len()
}

/// Rank items against a query. Tiers (best first): exact match on any field, prefix on any field, substring on any
/// field, every query word present across all fields, subsequence on any field. Ties keep input order. Items that
/// match nothing are left out; an empty query returns []. Callers slice the result.
pub fn rank<'a, T>(q: &str, items: &'a [T], fields: impl Fn(&T) -> Vec<String>) -> Vec<&'a T> {
    let q = norm(q);
    if q.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = q.split(crate::env::js_space).filter(|w| !w.is_empty()).collect();
    let compact: Vec<char> = q.chars().filter(|c| !crate::env::js_space(*c)).collect();
    let mut tiers: [Vec<&'a T>; 5] = Default::default();

    for item in items {
        let fs: Vec<String> = fields(item).iter().map(|f| norm(f)).filter(|f| !f.is_empty()).collect();
        if fs.is_empty() {
            continue;
        }
        let tier = if fs.iter().any(|f| *f == q) {
            Some(0)
        } else if fs.iter().any(|f| f.starts_with(q.as_str())) {
            Some(1)
        } else if fs.iter().any(|f| f.contains(q.as_str())) {
            Some(2)
        } else {
            let joined = fs.join(" ");
            if words.iter().all(|w| joined.contains(w)) {
                Some(3)
            } else if fs.iter().any(|f| is_subsequence(&compact, f)) {
                Some(4)
            } else {
                None
            }
        };
        if let Some(t) = tier {
            tiers[t].push(item);
        }
    }
    tiers.into_iter().flatten().collect()
}
