//! Review orchestration. Ported so far: resolve target → fetch diff → run
//! agent(s) → resolve → filter → emit `--format json`. Pending: inject backends,
//! consensus merge, `--bg`, parallel agents.
use crate::cli::ReviewArgs;
use crate::config::Config;
use crate::finding::Finding;
use crate::{agent, prompt, resolve, target};
use anyhow::{bail, Result};

pub async fn run(target_str: &str, args: &ReviewArgs, cfg: &Config) -> Result<()> {
    let format = args
        .format
        .clone()
        .or_else(|| std::env::var("SNIFFR_FORMAT").ok())
        .unwrap_or_else(|| "inject".into());

    let consensus_on = args.consensus
        || std::env::var("SNIFFR_CONSENSUS").is_ok()
        || cfg.consensus.model.is_some()
        || std::env::var("SNIFFR_CONSENSUS_MODEL").is_ok();

    // Ports still pending — fall back to the bash for these paths.
    if consensus_on {
        bail!("consensus isn't ported to the Rust build yet — use the bash `bin/sniffr`");
    }
    if format != "json" {
        bail!("inject mode isn't ported to the Rust build yet — use `--format json`, or the bash `bin/sniffr`");
    }

    let tgt = target::parse(target_str)?;
    let diff = target::diff(&tgt).await?;
    if diff.trim().is_empty() {
        println!("[]");
        return Ok(());
    }

    let agents = resolve_agents(args, cfg);
    let model = args
        .model
        .clone()
        .or_else(|| std::env::var("SNIFFR_MODEL").ok())
        .or_else(|| cfg.model.clone())
        .filter(|s| !s.is_empty());
    let max = cfg.max.unwrap_or(8);
    let full = prompt::build(&prompt::review_instructions(cfg), max, &diff);

    // sequential for now (parallel comes with consensus)
    let mut pooled: Vec<Finding> = Vec::new();
    for ag in &agents {
        let out = agent::run_agent(ag, model.as_deref(), &full).await.unwrap_or_default();
        pooled.extend(resolve::resolve(&diff, &out, ag));
    }

    let pooled = apply_filters(pooled, args, cfg);
    println!("{}", serde_json::to_string_pretty(&pooled)?);
    Ok(())
}

fn resolve_agents(args: &ReviewArgs, cfg: &Config) -> Vec<String> {
    // SNIFFR_CMD is a single command → one agent slot
    if std::env::var("SNIFFR_CMD").is_ok() {
        return vec!["custom".into()];
    }
    let raw = args
        .agent
        .clone()
        .or_else(|| std::env::var("SNIFFR_AGENT").ok())
        .unwrap_or_else(|| "codex".into());
    let _ = cfg; // saved-agent file could be read here later
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
        .or_else(|| cfg.min_severity.clone());
    let min_conf = args
        .min_confidence
        .or_else(|| std::env::var("SNIFFR_MIN_CONFIDENCE").ok().and_then(|s| s.parse().ok()))
        .or(cfg.min_confidence);
    let min_rank = severity_rank(min_sev.as_deref());
    findings
        .into_iter()
        .filter(|f| {
            // missing field is kept (never silently drop an unranked bug)
            let sev_ok = min_sev.is_none() || f.severity.is_none() || severity_rank(f.severity.as_deref()) >= min_rank;
            let conf_ok = min_conf.is_none_or(|mc| f.confidence.is_none_or(|c| c >= mc));
            sev_ok && conf_ok
        })
        .collect()
}
