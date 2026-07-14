//! `sniffr update` — git-pull the installed clone. (A cargo-dist self-updater
//! will replace this for release binaries; git-pull covers a source checkout.)
use anyhow::{bail, Result};
use std::path::PathBuf;
use std::process::Command;

fn install_root() -> PathBuf {
    std::env::var_os("SNIFFR_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share/sniffr"))
}

fn git(root: &PathBuf, args: &[&str]) -> String {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn run() -> Result<()> {
    let root = install_root();
    if !root.join(".git").is_dir() {
        eprintln!("sniffr: {} isn't a git checkout — reinstall to update:", root.display());
        eprintln!("  curl -fsSL https://raw.githubusercontent.com/tomasvarga/sniffr/main/install.sh | bash");
        std::process::exit(1);
    }
    let before = git(&root, &["rev-parse", "--short", "HEAD"]);
    println!("sniffr: updating {} …", root.display());
    let status = Command::new("git").arg("-C").arg(&root).args(["pull", "--ff-only"]).status()?;
    if !status.success() {
        bail!("git pull failed (local changes? cd {} && git status)", root.display());
    }
    let after = git(&root, &["describe", "--tags", "--always"]);
    if before == git(&root, &["rev-parse", "--short", "HEAD"]) {
        println!("sniffr: already up to date ({after}).");
    } else {
        println!("sniffr: updated to {after}.");
    }
    Ok(())
}
