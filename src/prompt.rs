//! Prompts. Defaults are embedded at compile time (self-contained binary);
//! config / env override them.
use crate::config::Config;

const DEFAULT_REVIEW: &str = include_str!("../prompts/review.md");
const DEFAULT_CONSENSUS: &str = include_str!("../prompts/consensus.md");

/// Review instructions: SNIFFR_PROMPT env → config `prompt` → embedded default.
pub fn review_instructions(cfg: &Config) -> String {
    std::env::var("SNIFFR_PROMPT")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| cfg.prompt.clone())
        .unwrap_or_else(|| DEFAULT_REVIEW.to_string())
        .trim()
        .to_string()
}

/// Consensus merge instructions: env → config → embedded default.
pub fn consensus_instructions(cfg: &Config) -> String {
    std::env::var("SNIFFR_CONSENSUS_PROMPT")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| cfg.consensus.prompt.clone())
        .unwrap_or_else(|| DEFAULT_CONSENSUS.to_string())
        .trim()
        .to_string()
}

/// Full agent prompt = instructions + the FIXED output contract + the diff.
pub fn build(instructions: &str, max: u32, diff: &str) -> String {
    format!(
        "{instructions}\n\n\
Output ONLY a JSON array — no prose, no markdown fences. Each item:\n\
{{\"file\":\"<path on the new side>\",\"code\":\"<the ONE line this finding is about, copied VERBATIM from a '+' or context line in the diff, WITHOUT the leading +/space>\",\"line\":<new-side line number if you are certain, else null>,\"type\":\"bug|risk|question|note\",\"severity\":\"critical|high|medium|low\",\"confidence\":<your certainty from 0.0 to 1.0>,\"body\":\"<concise finding>\",\"recommendation\":\"<the concrete fix in one line, or omit>\"}}\n\
The \"code\" field is what locates the line — copy it character-for-character from the diff; do NOT paraphrase, reindent, or reformat it. At most {max} items. If nothing significant, output [].\n\n\
DIFF:\n{diff}"
    )
}
