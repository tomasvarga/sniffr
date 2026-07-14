//! Consensus — a cheap model merges the pooled multi-agent findings into one
//! deduped finding per bug (stamped with the agents that agreed).
use crate::config::Config;
use crate::finding::Finding;
use crate::{agent, prompt, resolve};
use anyhow::Result;

/// Merge `pool` via `[consensus].model` (env overrides). Returns the merged
/// findings; the caller decides the raw-pool fallback if this comes back empty.
pub async fn merge(pool: &[Finding], default_agent: &str, cfg: &Config) -> Result<Vec<Finding>> {
    let model = std::env::var("SNIFFR_CONSENSUS_MODEL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| cfg.consensus.model.clone());
    let cagent = std::env::var("SNIFFR_CONSENSUS_AGENT")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| cfg.consensus.agent.clone())
        .unwrap_or_else(|| default_agent.to_string());

    let instructions = prompt::consensus_instructions(cfg);
    let pool_json = serde_json::to_string(pool)?;
    let full = format!("{instructions}\n\nFINDINGS:\n{pool_json}");

    let out = agent::run_agent(&cagent, model.as_deref(), &full)
        .await
        .unwrap_or_default();
    Ok(resolve::extract_array::<Finding>(&out))
}
