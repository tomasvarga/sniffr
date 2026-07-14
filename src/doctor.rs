//! `sniffr doctor` — preflight checks.
use crate::config::Config;
use owo_colors::OwoColorize;
use std::process::Command;
use which::which;

fn ok(msg: &str) {
    println!("  {} {msg}", "✓".green());
}
fn no(msg: &str) {
    println!("  {} {msg}", "✗".red());
}

const AGENTS: [&str; 6] = ["codex", "claude", "cursor-agent", "grok", "opencode", "ollama"];

pub fn run() -> anyhow::Result<()> {
    println!("sniffr doctor");
    let mut missing = false;

    println!("environment:");
    if which("gh").is_ok()
        && Command::new("gh")
            .args(["auth", "status"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    {
        ok("gh authenticated");
    } else {
        no("gh not authenticated — run: gh auth login");
        missing = true;
    }

    println!("review agent:");
    let found: Vec<&str> = AGENTS.iter().copied().filter(|a| which(a).is_ok()).collect();
    if found.is_empty() {
        no("no agent CLI found (codex/claude/cursor-agent/grok/opencode/ollama)");
        missing = true;
    } else {
        ok(&format!("agents available: {}", found.join(", ")));
    }

    println!("backend:");
    let cfg = Config::load();
    let backend = cfg.backend.clone().unwrap_or_else(|| {
        if which("tuicr").is_ok() {
            "tuicr".into()
        } else if which("hunk").is_ok() {
            "hunk".into()
        } else {
            "tuicr".into()
        }
    });
    match backend.as_str() {
        "tuicr" if which("tuicr").is_ok() => ok("tuicr (native)"),
        "hunk" if which("hunk").is_ok() => ok("hunk (native)"),
        "tuicr" | "hunk" => {
            no(&format!("backend '{backend}' selected but its binary isn't installed"));
            missing = true;
        }
        other => match cfg.backends.get(other).and_then(|b| b.inject.as_ref()) {
            Some(inj) => ok(&format!("custom backend '{other}' (inject: {inj})")),
            None => {
                no(&format!("backend '{other}' is neither native nor defined in config"));
                missing = true;
            }
        },
    }

    println!();
    if missing {
        println!("some checks failed (see ✗ above).");
        std::process::exit(1);
    }
    println!("all good — sniffr is ready.");
    Ok(())
}
