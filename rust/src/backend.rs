//! Backends — where findings go. Native: tuicr, hunk. Custom: config open/inject.
//! Attach mode: the reviewer is ALREADY OPEN; we discover its live session and
//! inject. `open_command` prints how a host wrapper (herdr-sniffr) opens it.
use crate::config::Config;
use crate::finding::Finding;
use crate::target::Target;
use anyhow::{bail, Context, Result};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Resolve the backend name: --backend > SNIFFR_BACKEND > config > auto-detect.
pub fn resolve(cfg: &Config, opt: Option<&str>) -> String {
    if let Some(b) = opt.filter(|s| !s.is_empty()) {
        return b.to_string();
    }
    if let Ok(b) = std::env::var("SNIFFR_BACKEND") {
        if !b.is_empty() {
            return b;
        }
    }
    if let Some(b) = &cfg.backend {
        return b.clone();
    }
    if which::which("tuicr").is_ok() {
        "tuicr".into()
    } else if which::which("hunk").is_ok() {
        "hunk".into()
    } else {
        "tuicr".into()
    }
}

pub fn is_known(cfg: &Config, name: &str) -> bool {
    matches!(name, "tuicr" | "hunk") || cfg.backends.contains_key(name)
}

/// The command that launches the resolved backend's viewer for `tgt`.
/// Writes the diff to `patch_path` for backends that need a file (hunk/custom).
pub async fn open_command(name: &str, tgt: &Target, patch_path: &str, diff: &str, cfg: &Config) -> Result<Option<String>> {
    match name {
        "tuicr" if tgt.local => Ok(Some(tuicr_local_open(tgt))),
        "tuicr" => Ok(Some(format!("tuicr pr {}#{}", tgt.repo, tgt.num))),
        "hunk" => {
            std::fs::write(patch_path, diff).with_context(|| format!("writing {patch_path}"))?;
            Ok(Some(format!("hunk patch '{patch_path}' --agent-notes --watch")))
        }
        other => match cfg.backends.get(other).and_then(|b| b.open.clone()) {
            Some(open) => {
                std::fs::write(patch_path, diff).ok();
                Ok(Some(expand(&open, tgt, patch_path)))
            }
            None => Ok(None),
        },
    }
}

/// Attach to the live session and inject `findings` (stamped, badged). Returns
/// how many landed. Fires `after_inject` (tuicr reload hook) at the end.
pub async fn inject(
    name: &str,
    tgt: &Target,
    patch_path: &str,
    findings: &[Finding],
    after_inject: Option<&str>,
    cfg: &Config,
) -> Result<usize> {
    match name {
        "tuicr" => {
            // Discover the live session slug: PR sessions match tgt.slug; local
            // (working-tree/range) sessions are found by `kind:"local"` for the checkout.
            let session = if tgt.local {
                match wait_for_local_session(&tgt.repo).await {
                    Some(s) => s,
                    None => bail!("no open local tuicr session for {} — open one first (tuicr -w), or use --format json", tgt.repo),
                }
            } else {
                if !wait_for(|| tuicr_has_session(&tgt.repo, &tgt.slug)).await {
                    bail!("no open tuicr session for {} — open the reviewer first (tuicr pr {}#{}), or use --format json", tgt.slug, tgt.repo, tgt.num);
                }
                tgt.slug.clone()
            };
            let mut n = 0;
            for f in findings {
                let Some(line) = f.line else { continue };
                let ok = Command::new("tuicr")
                    .args([
                        "review", "add", "--session", &session, "--repo", &tgt.repo,
                        "--type", &f.kind, "--target-file", &f.file, "--line", &line.to_string(),
                        "--side", "new", "--username", &f.author(), &f.badge(),
                    ])
                    .output().await.map(|o| o.status.success()).unwrap_or(false);
                if ok { n += 1; }
            }
            if let Some(cmd) = after_inject {
                let _ = Command::new("sh").arg("-c").arg(cmd).status().await;
            }
            Ok(n)
        }
        "hunk" => {
            let mut sid = String::new();
            for _ in 0..45 {
                if let Some(s) = hunk_session(patch_path).await {
                    sid = s;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(700)).await;
            }
            if sid.is_empty() {
                bail!("no open hunk session for {patch_path} — open `hunk patch` first, or use --format json");
            }
            let comments: Vec<serde_json::Value> = findings
                .iter()
                .filter_map(|f| {
                    let line = f.line?;
                    Some(serde_json::json!({
                        "filePath": f.file, "newLine": line, "summary": f.badge(), "author": f.author()
                    }))
                })
                .collect();
            if comments.is_empty() {
                return Ok(0);
            }
            let n = comments.len();
            let batch = serde_json::json!({ "comments": comments }).to_string();
            let mut child = Command::new("hunk")
                .args(["session", "comment", "apply", &sid, "--stdin"])
                .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
                .spawn().context("hunk comment apply")?;
            child.stdin.take().unwrap().write_all(batch.as_bytes()).await?;
            child.wait().await?;
            Ok(n)
        }
        other => {
            let inj = cfg.backends.get(other).and_then(|b| b.inject.clone());
            let Some(inj) = inj else { bail!("backend '{other}' has no inject command") };
            let json = serde_json::to_string(findings)?;
            let cmd = expand(&inj, tgt, patch_path);
            let mut child = Command::new("sh").arg("-c").arg(&cmd)
                .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
                .spawn().context("custom inject")?;
            child.stdin.take().unwrap().write_all(json.as_bytes()).await?;
            child.wait().await?;
            Ok(findings.len())
        }
    }
}

fn expand(s: &str, tgt: &Target, patch: &str) -> String {
    s.replace("{url}", &tgt.url)
        .replace("{repo}", &tgt.repo)
        .replace("{num}", &tgt.num)
        .replace("{diff}", patch)
}

async fn tuicr_has_session(repo: &str, slug: &str) -> bool {
    let out = Command::new("tuicr").args(["review", "list", "--repo", repo]).output().await;
    match out {
        Ok(o) => serde_json::from_slice::<serde_json::Value>(&o.stdout)
            .ok()
            .and_then(|v| v.as_array().map(|a| a.iter().any(|s| s.get("slug").and_then(|x| x.as_str()) == Some(slug))))
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// The command that opens a local tuicr session matching `tgt`'s diff source.
fn tuicr_local_open(tgt: &Target) -> String {
    use crate::target::Source;
    match &tgt.source {
        // ["diff","HEAD"] / ["diff","--cached"] → working tree; ["diff",<range>] → range
        Source::Git(args) => match args.get(1).map(String::as_str) {
            Some("HEAD") | Some("--cached") | None => "tuicr -w".into(),
            Some(range) => format!("tuicr -r {range}"),
        },
        _ => "tuicr -w".into(),
    }
}

/// Poll for a live local (`kind:"local"`) tuicr session for this checkout (~31s).
async fn wait_for_local_session(repo_path: &str) -> Option<String> {
    for _ in 0..45 {
        if let Some(s) = tuicr_local_session(repo_path).await {
            return Some(s);
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
    None
}

/// The most-recently-updated local session slug for `repo_path` (a checkout path).
async fn tuicr_local_session(repo_path: &str) -> Option<String> {
    let out = Command::new("tuicr").args(["review", "list", "--repo", repo_path]).output().await.ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v.as_array()?
        .iter()
        .filter(|s| s.get("kind").and_then(|k| k.as_str()) == Some("local"))
        .max_by_key(|s| s.get("updated_at").and_then(|u| u.as_str()).unwrap_or("").to_owned())
        .and_then(|s| s.get("slug").and_then(|x| x.as_str()).map(String::from))
}

async fn hunk_session(patch: &str) -> Option<String> {
    let out = Command::new("hunk").args(["session", "list", "--json"]).output().await.ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v.get("sessions")?.as_array()?.iter().find_map(|s| {
        if s.get("sourceLabel").and_then(|x| x.as_str()) == Some(patch) {
            s.get("sessionId").and_then(|x| x.as_str()).map(String::from)
        } else {
            None
        }
    })
}

/// Poll `cond` up to ~31s (45 × 0.7s).
async fn wait_for<F, Fut>(mut cond: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..45 {
        if cond().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
    false
}
