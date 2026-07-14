//! The two JSON shapes from docs/CONTRACT.md.
use serde::{Deserialize, Serialize};

/// What an agent (or SNIFFR_CMD) prints: line located by the verbatim `code` quote.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentFinding {
    pub file: Option<String>,
    pub code: Option<String>,
    pub line: Option<u32>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub severity: Option<String>,
    pub confidence: Option<f64>,
    pub body: Option<String>,
    pub recommendation: Option<String>,
}

/// A resolved finding: `--format json` emits these; backends render them.
/// `agent` for a single reviewer (raw); `agents` for a merged (consensus) finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<String>,
}

impl Finding {
    /// Comment author stamp: "consensus" for a merged finding, else the agent.
    pub fn author(&self) -> String {
        if self.agents.is_empty() {
            self.agent.clone().unwrap_or_else(|| "agent".into())
        } else {
            "consensus".into()
        }
    }

    /// Severity icon for the badge (matches the bash mkbadge).
    pub fn icon(&self) -> &'static str {
        match self.severity.as_deref() {
            Some("critical") => "🔴",
            Some("high") => "🟠",
            Some("medium") => "🟡",
            Some("low") => "⚪",
            _ => "·",
        }
    }

    /// The rendered comment body: "<icon> <sev> · <type>[ · N agents (chips)]" + body + fix.
    pub fn badge(&self) -> String {
        let mut head = match &self.severity {
            Some(s) => format!("{} {} · {}", self.icon(), s, self.kind),
            None => self.kind.clone(),
        };
        if !self.agents.is_empty() {
            let n = self.agents.len();
            head.push_str(&format!(
                " · {} agent{} ({})",
                n,
                if n == 1 { "" } else { "s" },
                self.agents.join("·")
            ));
        }
        let mut out = format!("{head}\n{}", self.body);
        if let Some(rec) = self.recommendation.as_deref().filter(|r| !r.is_empty()) {
            out.push_str(&format!("\n↳ fix: {rec}"));
        }
        out
    }
}
