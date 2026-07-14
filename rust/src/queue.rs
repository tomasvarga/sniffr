//! `sniffr queue` — pick from the PRs awaiting your review, then sniff it.
//! A target picker only: the choice is handed to `review::run`, so it can't drift.
use crate::cli::QueueArgs;
use crate::config::Config;
use crate::review;
use anyhow::{bail, Result};
use serde::Deserialize;
use std::io::Write;
use std::process::Command;

#[derive(Deserialize)]
struct Pr {
    repository: Repo,
    number: u64,
    title: String,
    author: Author,
    #[serde(rename = "isDraft", default)]
    is_draft: bool,
}
#[derive(Deserialize)]
struct Repo {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
}
#[derive(Deserialize)]
struct Author {
    login: String,
}

pub async fn run(q: &QueueArgs, cfg: &Config) -> Result<()> {
    let mut args: Vec<String> = vec![
        "search".into(), "prs".into(), "--review-requested=@me".into(), "--state=open".into(),
        "--json".into(), "repository,number,title,author,isDraft".into(),
        "--limit".into(), "40".into(),
    ];
    if !q.all {
        if let Some(d) = date_filter(&q.since) {
            args.push(format!("--updated=>={d}"));
        }
    }
    let out = Command::new("gh").args(&args).output()?;
    if !out.status.success() {
        bail!("gh search failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let mut prs: Vec<Pr> = serde_json::from_slice(&out.stdout).unwrap_or_default();

    if !q.all {
        const BOTS: [&str; 4] = ["dependabot", "renovate", "github-actions", "[bot]"];
        prs.retain(|p| {
            (q.include_bots || !BOTS.iter().any(|b| p.author.login.contains(b)))
                && (q.drafts || !p.is_draft)
        });
    }
    if prs.is_empty() {
        println!("sniffr queue: nothing needs your review{}.", if q.all { "" } else { " (recent · human · ready)" });
        return Ok(());
    }
    for (i, p) in prs.iter().enumerate() {
        println!("  {:>2}. {}#{}  {}", i + 1, p.repository.name_with_owner, p.number, p.title);
    }
    print!("pick [1-{}] (q to cancel): ", prs.len());
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let line = line.trim();
    if line.is_empty() || line.eq_ignore_ascii_case("q") {
        return Ok(());
    }
    let idx = line
        .parse::<usize>()
        .ok()
        .filter(|&n| (1..=prs.len()).contains(&n))
        .map(|n| n - 1)
        .ok_or_else(|| anyhow::anyhow!("invalid choice: {line}"))?;
    let chosen = &prs[idx];
    let target = format!("{}#{}", chosen.repository.name_with_owner, chosen.number);
    println!("sniffr: → {target}");
    review::run(&target, &q.review, cfg).await
}

/// since (21d|3w|2mo|1y) → YYYY-MM-DD via the `date` binary (BSD then GNU).
fn date_filter(since: &str) -> Option<String> {
    let (num, bsd, gnu) = parse_since(since)?;
    // BSD / macOS
    if let Ok(o) = Command::new("date").args([&format!("-v-{num}{bsd}"), "+%Y-%m-%d"]).output() {
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    // GNU / Linux
    if let Ok(o) = Command::new("date").args(["-d", &format!("{num} {gnu} ago"), "+%Y-%m-%d"]).output() {
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

fn parse_since(spec: &str) -> Option<(u32, &'static str, &'static str)> {
    let (num_str, bsd, gnu) = if let Some(n) = spec.strip_suffix("mo") {
        (n, "m", "months")
    } else if let Some(n) = spec.strip_suffix('d') {
        (n, "d", "days")
    } else if let Some(n) = spec.strip_suffix('w') {
        (n, "w", "weeks")
    } else if let Some(n) = spec.strip_suffix('y') {
        (n, "y", "years")
    } else {
        return None;
    };
    num_str.parse::<u32>().ok().map(|n| (n, bsd, gnu))
}
