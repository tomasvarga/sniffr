//! Parse a PR target (number | owner/repo#N | URL) and fetch its diff via `gh`.
use anyhow::{bail, Context, Result};
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct Target {
    pub repo: String,
    pub num: String,
    pub slug: String,
    pub url: String,
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
    } else {
        bail!("bare number needs owner/repo#N or a URL: {t}");
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
    })
}

pub async fn diff(t: &Target) -> Result<String> {
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
