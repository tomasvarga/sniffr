//! Port of the Python resolver: extract the agent's JSON array, then anchor each
//! finding to a real new-side line by matching its verbatim `code` quote against
//! the diff (the agent's `line` is only a tiebreaker).
use crate::finding::{AgentFinding, Finding};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

type NewLines = Vec<(u32, String)>; // (new-side line no, text without +/space)

/// Resolve one agent's raw output against the diff into stamped findings.
pub fn resolve(diff: &str, agent_output: &str, agent: &str) -> Vec<Finding> {
    let items: Vec<AgentFinding> = extract_array(agent_output);
    let files = parse_diff(diff);
    let mut out = Vec::new();
    for it in items {
        let body = it.body.unwrap_or_default();
        let body = body.trim();
        let fpath = it.file.unwrap_or_default();
        let fpath = fpath.trim();
        if body.is_empty() || fpath.is_empty() {
            continue;
        }
        let fkey = match_file(&files, fpath);
        let line = resolve_line(&files, fkey.as_deref(), it.code.as_deref(), it.line);
        let severity = it
            .severity
            .map(|s| s.trim().to_lowercase())
            .filter(|s| matches!(s.as_str(), "critical" | "high" | "medium" | "low"));
        let confidence = it
            .confidence
            .filter(|c| (0.0..=1.0).contains(c))
            .map(|c| (c * 100.0).round() / 100.0);
        out.push(Finding {
            file: fkey.unwrap_or_else(|| fpath.to_string()),
            line,
            kind: it.kind.filter(|k| !k.is_empty()).unwrap_or_else(|| "note".into()),
            severity,
            confidence,
            body: body.to_string(),
            recommendation: it.recommendation.map(|r| r.trim().to_string()).filter(|r| !r.is_empty()),
            agent: Some(agent.to_string()),
            agents: Vec::new(),
        });
    }
    out
}

/// Pull the largest JSON array-of-`T` out of a model's stdout (handles prose or
/// fences around it). Used for agent output (AgentFinding) and merged (Finding).
pub fn extract_array<T: serde::de::DeserializeOwned>(s: &str) -> Vec<T> {
    if let Ok(v) = serde_json::from_str::<Vec<T>>(s.trim()) {
        return v;
    }
    let bytes = s.as_bytes();
    let mut best: Vec<T> = Vec::new();
    for (i, _) in s.match_indices('[') {
        // find this '['s matching ']' with bracket depth that skips over JSON
        // string contents, so a `code` field like "arr[0]" can't fool the scan
        if let Some(j) = balanced_end(bytes, i) {
            if let Ok(v) = serde_json::from_str::<Vec<T>>(&s[i..j]) {
                if v.len() >= best.len() {
                    best = v;
                }
            }
        }
    }
    best
}

/// Given `open` = byte index of a `[`, return the index just past its matching
/// `]`. Tracks nesting but ignores brackets inside JSON strings (respecting `\`
/// escapes). `None` if the bracket never closes.
fn balanced_end(bytes: &[u8], open: usize) -> Option<usize> {
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        if in_str {
            match b {
                _ if esc => esc = false,
                b'\\' => esc = true,
                b'"' => in_str = false,
                _ => {}
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn parse_diff(diff: &str) -> HashMap<String, NewLines> {
    static HUNK: OnceLock<Regex> = OnceLock::new();
    let hunk = HUNK.get_or_init(|| Regex::new(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@").unwrap());

    let mut files: HashMap<String, NewLines> = HashMap::new();
    let mut cur: Option<String> = None;
    let mut newno: Option<u32> = None;

    for ln in diff.split('\n') {
        if ln.starts_with("diff --git") {
            // new file header — drop context until the next +++ so `diff --git`/
            // `index` lines can't be mistaken for context of the previous file
            cur = None;
            newno = None;
        } else if let Some(rest) = ln.strip_prefix("+++ ") {
            let p = rest.split('\t').next().unwrap_or("").trim();
            let p = p.strip_prefix("b/").unwrap_or(p);
            cur = if p == "/dev/null" {
                None
            } else {
                files.entry(p.to_string()).or_default();
                Some(p.to_string())
            };
            newno = None;
        } else if ln.starts_with("--- ") {
            continue;
        } else if let Some(c) = hunk.captures(ln) {
            newno = c.get(1).and_then(|m| m.as_str().parse().ok());
        } else if cur.is_none() || newno.is_none() {
            continue;
        } else if let Some(t) = ln.strip_prefix('+') {
            let (c, n) = (cur.clone().unwrap(), newno.unwrap());
            files.get_mut(&c).unwrap().push((n, t.to_string()));
            newno = Some(n + 1);
        } else if ln.starts_with('-') || ln.starts_with('\\') {
            // removed / "\ No newline" — no new-side line
        } else {
            let t = ln.strip_prefix(' ').unwrap_or(ln);
            let (c, n) = (cur.clone().unwrap(), newno.unwrap());
            files.get_mut(&c).unwrap().push((n, t.to_string()));
            newno = Some(n + 1);
        }
    }
    files
}

fn match_file(files: &HashMap<String, NewLines>, fpath: &str) -> Option<String> {
    if fpath.is_empty() {
        return None;
    }
    if files.contains_key(fpath) {
        return Some(fpath.to_string());
    }
    for k in files.keys() {
        if k.ends_with(fpath) || fpath.ends_with(k.as_str()) {
            return Some(k.clone());
        }
    }
    let base = Path::new(fpath).file_name()?.to_str()?;
    let cands: Vec<&String> = files
        .keys()
        .filter(|k| Path::new(k).file_name().and_then(|b| b.to_str()) == Some(base))
        .collect();
    if cands.len() == 1 {
        Some(cands[0].clone())
    } else {
        None
    }
}

fn resolve_line(files: &HashMap<String, NewLines>, fkey: Option<&str>, code: Option<&str>, hint: Option<u32>) -> Option<u32> {
    let lines = files.get(fkey.unwrap_or(""))?;
    if lines.is_empty() {
        return None;
    }
    let c = code.unwrap_or("").trim();
    if !c.is_empty() {
        let exact: Vec<u32> = lines.iter().filter(|(_, t)| t.trim() == c).map(|(n, _)| *n).collect();
        if exact.len() == 1 {
            return Some(exact[0]);
        }
        if exact.len() > 1 {
            return Some(nearest(&exact, hint));
        }
        let sub: Vec<u32> = lines
            .iter()
            .filter(|(_, t)| !t.trim().is_empty() && (t.trim().contains(c) || c.contains(t.trim())))
            .map(|(n, _)| *n)
            .collect();
        if sub.len() == 1 {
            return Some(sub[0]);
        }
        if sub.len() > 1 {
            if let Some(h) = hint {
                return Some(nearest(&sub, Some(h)));
            }
        }
    }
    if let Some(h) = hint {
        let (min, max) = (lines.iter().map(|(n, _)| *n).min()?, lines.iter().map(|(n, _)| *n).max()?);
        if (min..=max).contains(&h) {
            return Some(h);
        }
    }
    None
}

fn nearest(candidates: &[u32], hint: Option<u32>) -> u32 {
    match hint {
        Some(h) => *candidates.iter().min_by_key(|&&n| n.abs_diff(h)).unwrap(),
        None => candidates[0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A two-file diff: src/a.rs adds lines at new-side 10-12; src/b.rs at 5-6.
    const DIFF: &str = "\
diff --git a/src/a.rs b/src/a.rs
index 111..222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -9,3 +9,4 @@ fn f() {
 let keep = 1;
+let added = compute(idx);
+let other = 2;
 let tail = 3;
diff --git a/src/b.rs b/src/b.rs
index 333..444 100644
--- a/src/b.rs
+++ b/src/b.rs
@@ -4,2 +4,3 @@
 ctx;
+let dupe = 0;
";

    #[test]
    fn parse_diff_tracks_new_side_lines_per_file() {
        let files = parse_diff(DIFF);
        let a = files.get("src/a.rs").expect("a.rs present");
        // added + context on the new side, numbered from the hunk's +9
        assert_eq!(a[0], (9, "let keep = 1;".into()));
        assert_eq!(a[1], (10, "let added = compute(idx);".into()));
        assert_eq!(a[2], (11, "let other = 2;".into()));
        assert!(files.contains_key("src/b.rs"));
    }

    #[test]
    fn diff_git_and_index_lines_are_not_context() {
        // the `index`/`diff --git` junk between files must not leak into a.rs
        let files = parse_diff(DIFF);
        let a = files.get("src/a.rs").unwrap();
        assert!(!a.iter().any(|(_, t)| t.starts_with("index ") || t.starts_with("diff --git")));
    }

    #[test]
    fn match_file_falls_back_to_basename_and_suffix() {
        let files = parse_diff(DIFF);
        assert_eq!(match_file(&files, "src/a.rs").as_deref(), Some("src/a.rs"));
        assert_eq!(match_file(&files, "a.rs").as_deref(), Some("src/a.rs")); // basename
        assert_eq!(match_file(&files, "repo/src/b.rs").as_deref(), Some("src/b.rs")); // suffix
        assert_eq!(match_file(&files, "nope.rs"), None);
    }

    #[test]
    fn resolve_line_anchors_on_the_code_quote_not_the_hint() {
        let files = parse_diff(DIFF);
        // the hint (999) is out of range; the verbatim quote wins
        let n = resolve_line(&files, Some("src/a.rs"), Some("let other = 2;"), Some(999));
        assert_eq!(n, Some(11));
    }

    #[test]
    fn resolve_line_uses_hint_to_break_ties() {
        let files = parse_diff(DIFF);
        // "ctx;" appears once here; use a substring that could match, tie-broken by hint
        let n = resolve_line(&files, Some("src/a.rs"), Some("let"), Some(10));
        assert_eq!(n, Some(10)); // nearest of {10,11} to the hint
    }

    #[test]
    fn extract_array_ignores_brackets_inside_strings() {
        // a `code` field literally containing `]` used to truncate the naive scan
        let out = r#"prose before [{"file":"a.rs","body":"arr[0] = xs[i];","code":"] not real"}] trailing"#;
        let v: Vec<AgentFinding> = extract_array(out);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].file.as_deref(), Some("a.rs"));
        assert_eq!(v[0].code.as_deref(), Some("] not real"));
    }

    #[test]
    fn extract_array_picks_the_largest_array() {
        let out = r#"noise [1,2] then [{"file":"x"},{"file":"y"},{"file":"z"}]"#;
        let v: Vec<AgentFinding> = extract_array(out);
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn resolve_end_to_end_stamps_agent_and_defaults_kind() {
        let out = r#"[{"file":"src/a.rs","body":"off-by-one","code":"let added = compute(idx);","line":10}]"#;
        let found = resolve(DIFF, out, "codex");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, Some(10));
        assert_eq!(found[0].kind, "note"); // no "type" given → default
        assert_eq!(found[0].agent.as_deref(), Some("codex"));
    }
}
