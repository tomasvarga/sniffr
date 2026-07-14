//! Resolve what to review into a diff source: a GitHub PR (via `gh pr diff`), a
//! local git range / working tree (via `git diff`), or a unified diff on stdin.
use anyhow::{bail, Context, Result};
use std::io::Read;
use tokio::process::Command;

/// Where a diff comes from.
#[derive(Debug, Clone)]
pub enum Source {
    /// GitHub PR — `gh pr diff <num> --repo <repo>`.
    Pr,
    /// Local `git diff <args…>` (working tree, staged, or a ref range).
    Git(Vec<String>),
    /// A unified diff read from stdin.
    Stdin,
}

#[derive(Debug, Clone)]
pub struct Target {
    /// PR: `owner/repo`. Local: the checkout path (`.`) — the tuicr `--repo` selector.
    pub repo: String,
    /// PR number, or a local label (`worktree` / `staged` / a sanitized range).
    pub num: String,
    /// tuicr PR slug (`gh:owner/repo/pr/N`). Empty for local — discovered at inject.
    pub slug: String,
    pub url: String,
    pub source: Source,
    pub local: bool,
}

/// Resolve the CLI target + flags into a diff source.
pub fn resolve(target_str: Option<&str>, diff: bool, staged: bool) -> Result<Target> {
    // stdin: `-` (with or without --diff)
    if target_str == Some("-") {
        return Ok(local("-", "stdin", Source::Stdin));
    }
    // local working tree / staged
    if diff || staged {
        let (args, label) = if staged {
            (vec!["diff".into(), "--cached".into()], "staged")
        } else {
            (vec!["diff".into(), "HEAD".into()], "worktree")
        };
        return Ok(local(".", label, Source::Git(args)));
    }
    match target_str {
        // a git ref range (main..HEAD, a...b) → local git diff
        Some(t) if t.contains("..") => {
            Ok(local(".", &sanitize(t), Source::Git(vec!["diff".into(), t.into()])))
        }
        Some(t) => parse(t), // GitHub PR
        None => bail!("nothing to review — pass a PR (owner/repo#N | URL | number), a git range (main..HEAD), --diff for local changes, or - for a diff on stdin"),
    }
}

/// Build a local (non-PR) target. `repo` is the tuicr checkout selector.
fn local(repo: &str, label: &str, source: Source) -> Target {
    Target {
        repo: repo.to_string(),
        num: label.to_string(),
        slug: String::new(),
        url: String::new(),
        source,
        local: true,
    }
}

/// Filename-safe label for a ref range (`feature/x..HEAD` → `feature-x..HEAD`).
fn sanitize(range: &str) -> String {
    range.replace(['/', ' ', ':'], "-")
}

pub fn parse(t: &str) -> Result<Target> {
    let (repo, num) = if let Some(rest) = t.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 4 && parts[2] == "pull" {
            (format!("{}/{}", parts[0], parts[1]), parts[3].to_string())
        } else {
            bail!("unrecognized PR URL: {t}");
        }
    } else if let Some((repo, num)) = t.split_once('#') {
        (repo.to_string(), num.to_string())
    } else if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
        // bare PR number → resolve the repo from the cwd (like the bash `gh repo view`)
        let repo = gh_current_repo()
            .ok_or_else(|| anyhow::anyhow!("bare number needs a repo cwd; pass owner/repo#N or a URL"))?;
        (repo, t.to_string())
    } else {
        bail!("unrecognized PR target: {t}");
    };
    let num: String = num.chars().take_while(|c| c.is_ascii_digit()).collect();
    if num.is_empty() {
        bail!("no PR number in {t}");
    }
    Ok(Target {
        slug: format!("gh:{repo}/pr/{num}"),
        url: format!("https://github.com/{repo}/pull/{num}"),
        repo,
        num,
        source: Source::Pr,
        local: false,
    })
}

/// The `owner/repo` of the cwd's default remote, via `gh repo view`.
fn gh_current_repo() -> Option<String> {
    let out = std::process::Command::new("gh")
        .args(["repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Fetch the diff for `t` from its source.
pub async fn diff(t: &Target) -> Result<String> {
    match &t.source {
        Source::Pr => {
            let out = Command::new("gh")
                .args(["pr", "diff", &t.num, "--repo", &t.repo])
                .output()
                .await
                .context("running `gh pr diff`")?;
            if !out.status.success() {
                bail!("could not fetch diff for {}#{}", t.repo, t.num);
            }
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Source::Git(args) => {
            let out = Command::new("git")
                .args(args)
                .output()
                .await
                .context("running `git diff`")?;
            if !out.status.success() {
                bail!("`git diff` failed: {}", String::from_utf8_lossy(&out.stderr).trim());
            }
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Source::Stdin => {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s).context("reading diff from stdin")?;
            Ok(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_args(t: &Target) -> Vec<String> {
        match &t.source {
            Source::Git(a) => a.clone(),
            _ => panic!("expected a git source"),
        }
    }

    #[test]
    fn worktree_is_git_diff_head() {
        let t = resolve(None, true, false).unwrap();
        assert!(t.local && t.num == "worktree");
        assert_eq!(git_args(&t), ["diff", "HEAD"]);
    }

    #[test]
    fn staged_is_git_diff_cached() {
        let t = resolve(None, false, true).unwrap();
        assert!(t.local && t.num == "staged");
        assert_eq!(git_args(&t), ["diff", "--cached"]);
    }

    #[test]
    fn ref_range_is_local_git_diff() {
        let t = resolve(Some("main..HEAD"), false, false).unwrap();
        assert!(t.local);
        assert_eq!(git_args(&t), ["diff", "main..HEAD"]);
    }

    #[test]
    fn range_label_is_filename_safe() {
        let t = resolve(Some("feature/x..HEAD"), false, false).unwrap();
        assert_eq!(t.num, "feature-x..HEAD"); // '/' sanitized for the patch filename
    }

    #[test]
    fn dash_is_stdin() {
        let t = resolve(Some("-"), false, false).unwrap();
        assert!(t.local && matches!(t.source, Source::Stdin));
    }

    #[test]
    fn pr_shorthand_and_url_stay_pr() {
        let a = resolve(Some("owner/repo#42"), false, false).unwrap();
        assert!(!a.local && a.repo == "owner/repo" && a.num == "42");
        assert_eq!(a.slug, "gh:owner/repo/pr/42");
        assert!(matches!(a.source, Source::Pr));

        let b = resolve(Some("https://github.com/o/r/pull/7"), false, false).unwrap();
        assert_eq!((b.repo.as_str(), b.num.as_str()), ("o/r", "7"));
    }

    #[test]
    fn nothing_to_review_errors() {
        assert!(resolve(None, false, false).is_err());
    }
}
