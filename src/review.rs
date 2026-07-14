//! Review orchestration: resolve target → fetch diff → run agent(s) → resolve →
//! filter → (consensus merge) → inject into a live reviewer or emit JSON.
//! `--bg` re-spawns a detached worker (the tokio runtime dies when main returns).
use crate::cli::ReviewArgs;
use crate::config::Config;
use crate::finding::Finding;
use crate::{agent, backend, consensus, prompt, resolve, target};
use anyhow::{bail, Result};
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const MAX_PARALLEL: usize = 4;

pub async fn run(target_str: &str, args: &ReviewArgs, cfg: &Config) -> Result<()> {
    // --bg: detach a worker and return (unless we ARE the detached worker).
    if args.bg && std::env::var_os("SNIFFR_BG_WORKER").is_none() {
        return spawn_detached(target_str);
    }

    let format = args
        .format
        .clone()
        .or_else(|| std::env::var("SNIFFR_FORMAT").ok())
        .unwrap_or_else(|| "inject".into());
    if !matches!(format.as_str(), "inject" | "json") {
        bail!("unknown --format '{format}' (use: inject | json)");
    }
    let consensus_on = args.consensus
        || matches!(std::env::var("SNIFFR_CONSENSUS").ok().as_deref(), Some(v) if !v.is_empty() && v != "0" && v != "false")
        || cfg.consensus.model.is_some()
        || std::env::var("SNIFFR_CONSENSUS_MODEL").is_ok_and(|v| !v.is_empty());

    let tgt = target::parse(target_str)?;
    let diff = target::diff(&tgt).await?;
    if diff.trim().is_empty() {
        if format == "json" {
            println!("[]");
        } else {
            notify("sniffr", &format!("sniffed #{} and flagged nothing.", tgt.num));
        }
        return Ok(());
    }

    let agents = resolve_agents(args);
    let model = args
        .model
        .clone()
        .or_else(|| std::env::var("SNIFFR_MODEL").ok())
        .or_else(|| cfg.model.clone())
        .filter(|s| !s.is_empty());
    let max = cfg.max.unwrap_or(8);
    let full: Arc<str> = Arc::from(prompt::build(&prompt::review_instructions(cfg), max, &diff));
    let diff_arc: Arc<str> = Arc::from(diff.as_str());
    let backend_name = backend::resolve(cfg, args.backend.as_deref());
    if !backend::is_known(cfg, &backend_name) {
        bail!("unknown backend '{backend_name}'. Native: tuicr, hunk. Define custom backends under [backends.<name>] in the config.");
    }
    let patch = std::env::temp_dir()
        .join(format!("sniffr-{}.patch", tgt.num))
        .to_string_lossy()
        .into_owned();
    let after = args
        .after_inject
        .clone()
        .or_else(|| std::env::var("SNIFFR_AFTER_INJECT").ok())
        .filter(|s| !s.is_empty());

    if format == "json" || consensus_on {
        // pool all agents in parallel, then optionally merge
        let mut pooled = run_agents_parallel(&agents, model.clone(), full.clone(), diff_arc.clone()).await;
        if consensus_on {
            let default_agent = agents.first().cloned().unwrap_or_else(|| "codex".into());
            let merged = consensus::merge(&pooled, &default_agent, cfg).await?;
            if !merged.is_empty() || pooled.is_empty() {
                pooled = merged; // else keep the raw pool as a fallback
            } else {
                eprintln!("sniffr: consensus merge returned nothing — falling back to raw pooled findings.");
            }
        }
        let findings = apply_filters(pooled, args, cfg);
        if format == "json" {
            println!("{}", serde_json::to_string_pretty(&findings)?);
        } else {
            let n = backend::inject(&backend_name, &tgt, &patch, &findings, after.as_deref(), cfg).await?;
            notify_done(consensus_on, agents.len(), n, &tgt.num);
        }
    } else {
        // progressive inject: each agent's findings land as it finishes
        let mut total = 0usize;
        for ag in &agents {
            let out = agent::run_agent(ag, model.as_deref(), &full).await.unwrap_or_default();
            let f = apply_filters(resolve::resolve(&diff, &out, ag), args, cfg);
            total += backend::inject(&backend_name, &tgt, &patch, &f, after.as_deref(), cfg).await?;
        }
        notify_done(false, agents.len(), total, &tgt.num);
    }
    Ok(())
}

async fn run_agents_parallel(agents: &[String], model: Option<String>, full: Arc<str>, diff: Arc<str>) -> Vec<Finding> {
    let sem = Arc::new(Semaphore::new(MAX_PARALLEL));
    let mut set: JoinSet<(usize, Vec<Finding>)> = JoinSet::new();
    for (i, ag) in agents.iter().cloned().enumerate() {
        let (full, diff, sem, model) = (full.clone(), diff.clone(), sem.clone(), model.clone());
        set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore");
            let out = agent::run_agent(&ag, model.as_deref(), &full).await.unwrap_or_default();
            (i, resolve::resolve(&diff, &out, &ag))
        });
    }
    let mut results: Vec<(usize, Vec<Finding>)> = Vec::new();
    while let Some(joined) = set.join_next().await {
        if let Ok(pair) = joined {
            results.push(pair);
        }
    }
    results.sort_by_key(|(i, _)| *i); // deterministic order despite completion order
    results.into_iter().flat_map(|(_, f)| f).collect()
}

fn resolve_agents(args: &ReviewArgs) -> Vec<String> {
    if std::env::var_os("SNIFFR_CMD").is_some() {
        return vec!["custom".into()];
    }
    let raw = args
        .agent
        .clone()
        .or_else(|| std::env::var("SNIFFR_AGENT").ok())
        .unwrap_or_else(|| "codex".into());
    raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn severity_rank(s: Option<&str>) -> u8 {
    match s {
        Some("critical") => 4,
        Some("high") => 3,
        Some("medium") => 2,
        Some("low") => 1,
        _ => 0,
    }
}

fn apply_filters(findings: Vec<Finding>, args: &ReviewArgs, cfg: &Config) -> Vec<Finding> {
    let min_sev = args
        .min_severity
        .clone()
        .or_else(|| std::env::var("SNIFFR_MIN_SEVERITY").ok())
        .or_else(|| cfg.min_severity.clone())
        .filter(|s| !s.is_empty());
    let min_conf = args
        .min_confidence
        .or_else(|| std::env::var("SNIFFR_MIN_CONFIDENCE").ok().and_then(|s| s.parse().ok()))
        .or(cfg.min_confidence);
    let min_rank = severity_rank(min_sev.as_deref());
    findings
        .into_iter()
        .filter(|f| {
            let sev_ok = min_sev.is_none() || f.severity.is_none() || severity_rank(f.severity.as_deref()) >= min_rank;
            let conf_ok = min_conf.is_none_or(|mc| f.confidence.is_none_or(|c| c >= mc));
            sev_ok && conf_ok
        })
        .collect()
}

fn spawn_detached(target_str: &str) -> Result<()> {
    let exe = std::env::current_exe()?;
    // pass through argv minus the --bg flag; mark the child as the worker
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !matches!(a.as_str(), "--bg" | "--background" | "--async"))
        .collect();
    let logp = std::env::temp_dir().join("sniffr-bg.log");
    let log = std::fs::File::create(&logp)?;
    std::process::Command::new(exe)
        .args(&args)
        .env("SNIFFR_BG_WORKER", "1")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?;
    println!("sniffr: sniffing {target_str} in the background (log: {})", logp.display());
    Ok(())
}

fn notify_done(consensus: bool, nagents: usize, n: usize, num: &str) {
    if n > 0 {
        let title = if consensus { "sniffr — consensus ready" } else { "sniffr — review ready" };
        notify(title, &format!("{nagents} agent(s) → {n} comment(s) on PR #{num}."));
    } else {
        notify("sniffr", &format!("sniffed PR #{num} and flagged nothing."));
    }
}

/// Notify via SNIFFR_NOTIFY_CMD (env SNIFFR_TITLE/SNIFFR_MSG) → terminal-notifier → osascript.
fn notify(title: &str, body: &str) {
    use std::process::Command;
    if let Ok(cmd) = std::env::var("SNIFFR_NOTIFY_CMD") {
        if !cmd.is_empty() {
            let _ = Command::new("sh").arg("-c").arg(&cmd).env("SNIFFR_TITLE", title).env("SNIFFR_MSG", body).status();
            return;
        }
    }
    if which::which("terminal-notifier").is_ok() {
        let _ = Command::new("terminal-notifier").args(["-title", title, "-message", body]).status();
    } else if which::which("osascript").is_ok() {
        let script = format!("display notification \"{}\" with title \"{}\"", body.replace('"', ""), title.replace('"', ""));
        let _ = Command::new("osascript").args(["-e", &script]).status();
    }
}
