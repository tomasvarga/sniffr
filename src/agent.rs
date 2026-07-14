//! Running agent CLIs. Each shells out (like the bash); consensus runs them in
//! parallel. `SNIFFR_CMD` is the escape hatch (prompt on stdin → JSON on stdout).
use anyhow::{bail, Context, Result};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Run one agent over `prompt`; return its raw stdout (stderr discarded).
pub async fn run_agent(agent: &str, model: Option<&str>, prompt: &str) -> Result<String> {
    if let Ok(cmd) = std::env::var("SNIFFR_CMD") {
        return run(&["sh".into(), "-c".into(), cmd], Some(prompt)).await;
    }
    let m = model.filter(|s| !s.is_empty());
    let mut a: Vec<String> = Vec::new();
    match agent {
        "codex" => {
            a.extend(["exec".into(), "--skip-git-repo-check".into()]);
            if let Some(m) = m { a.extend(["--model".into(), m.into()]); }
            a.push("-".into());
            run(&prepend("codex", a), Some(prompt)).await
        }
        "claude" => {
            a.push("-p".into());
            if let Some(m) = m { a.extend(["--model".into(), m.into()]); }
            run(&prepend("claude", a), Some(prompt)).await
        }
        "cursor" | "cursor-agent" => {
            a.extend(["-f".into(), "-p".into()]);
            if let Some(m) = m { a.extend(["--model".into(), m.into()]); }
            a.extend(["--output-format".into(), "text".into(), prompt.into()]);
            run(&prepend("cursor-agent", a), None).await
        }
        "grok" => {
            a.push("-p".into());
            if let Some(m) = m { a.extend(["--model".into(), m.into()]); }
            a.push(prompt.into());
            run(&prepend("grok", a), None).await
        }
        "opencode" => {
            a.push("run".into());
            if let Some(m) = m { a.extend(["-m".into(), m.into()]); }
            a.push(prompt.into());
            run(&prepend("opencode", a), None).await
        }
        "ollama" => run(&["ollama".into(), "run".into(), m.unwrap_or("llama3.1").into()], Some(prompt)).await,
        other => bail!("unknown agent '{other}'"),
    }
}

fn prepend(bin: &str, mut rest: Vec<String>) -> Vec<String> {
    let mut v = vec![bin.to_string()];
    v.append(&mut rest);
    v
}

/// Spawn argv[0] with argv[1..]; optionally feed `stdin`; capture stdout.
async fn run(argv: &[String], stdin: Option<&str>) -> Result<String> {
    let mut cmd = Command::new(&argv[0]);
    // capture stderr so a crashed agent surfaces *why* instead of looking clean
    cmd.args(&argv[1..]).stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().with_context(|| format!("spawn {}", argv[0]))?;
    if let Some(s) = stdin {
        let mut si = child.stdin.take().expect("stdin piped");
        let owned = s.to_string();
        // write concurrently with reading stdout to avoid pipe deadlock
        tokio::spawn(async move {
            let _ = si.write_all(owned.as_bytes()).await;
            // si dropped here → stdin closed (EOF)
        });
    }
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        let tail = if err.is_empty() { String::new() } else { format!(": {err}") };
        bail!("agent '{}' exited unsuccessfully ({}){tail}", argv[0], out.status);
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
