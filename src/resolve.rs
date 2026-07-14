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
        let mut j = s.len();
        while j > i {
            if bytes[j - 1] == b']' {
                if let Ok(v) = serde_json::from_str::<Vec<T>>(&s[i..j]) {
                    if v.len() >= best.len() {
                        best = v;
                    }
                    break;
                }
            }
            j -= 1;
        }
    }
    best
}

fn parse_diff(diff: &str) -> HashMap<String, NewLines> {
    static HUNK: OnceLock<Regex> = OnceLock::new();
    let hunk = HUNK.get_or_init(|| Regex::new(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@").unwrap());

    let mut files: HashMap<String, NewLines> = HashMap::new();
    let mut cur: Option<String> = None;
    let mut newno: Option<u32> = None;

    for ln in diff.split('\n') {
        if let Some(rest) = ln.strip_prefix("+++ ") {
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
