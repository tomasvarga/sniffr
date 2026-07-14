//! Command-line surface — mirrors the bash `sniffr`.
use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "sniffr",
    version,
    about = "An AI sniffs your PR for issues before you review it.",
    long_about = "Runs an agent over a PR diff, anchors each finding to the exact changed \
                  line, and injects them as LOCAL-DRAFT comments into your open reviewer \
                  (tuicr/hunk/custom) — or prints them as JSON. sniffr never posts to GitHub."
)]
pub struct Cli {
    /// PR target — number | owner/repo#N | URL. Reviewing it is the default action.
    pub target: Option<String>,

    #[command(flatten)]
    pub review: ReviewArgs,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Pick from the PRs actually awaiting your review, then sniff the chosen one.
    Queue(QueueArgs),
    /// Print the command that launches the resolved backend's viewer (for host wrappers).
    OpenCmd(OpenCmdArgs),
    /// Preflight: deps, gh auth, agent, backend.
    Doctor,
    /// Print an agent setup prompt (pipe into a coding agent).
    Setup,
    /// git-pull the installed clone to the latest and re-link.
    Update,
    /// Print the installed version.
    Version,
}

/// Options for reviewing a PR (the default action).
#[derive(Args, Debug, Default)]
pub struct ReviewArgs {
    /// One agent, or a comma list (codex,claude,cursor,grok,opencode,ollama).
    #[arg(long)]
    pub agent: Option<String>,
    /// Where findings go: tuicr | hunk | <custom>. Default: auto-detect.
    #[arg(long)]
    pub backend: Option<String>,
    /// Model for the agent(s); unset = each agent's own default.
    #[arg(long)]
    pub model: Option<String>,
    /// Merge multi-agent findings into one comment per bug (needs [consensus].model or this flag).
    #[arg(long)]
    pub consensus: bool,
    /// Keep only findings at/above this severity: critical | high | medium | low.
    #[arg(long)]
    pub min_severity: Option<String>,
    /// Keep only findings at/above this confidence (0.0–1.0).
    #[arg(long)]
    pub min_confidence: Option<f64>,
    /// Review in the background; return immediately.
    #[arg(long, visible_aliases = ["background", "async"])]
    pub bg: bool,
    /// Output: inject (default) | json.
    #[arg(long)]
    pub format: Option<String>,
    /// Run <cmd> after findings land (e.g. reload the reviewer).
    #[arg(long)]
    pub after_inject: Option<String>,
    /// Run <cmd> to notify (env SNIFFR_TITLE / SNIFFR_MSG).
    #[arg(long)]
    pub notify_cmd: Option<String>,
}

#[derive(Args, Debug)]
pub struct QueueArgs {
    /// Activity window: 21d | 3w | 2mo | 1y.
    #[arg(long, default_value = "21d")]
    pub since: String,
    /// The full list: bots + drafts + no date cutoff.
    #[arg(long)]
    pub all: bool,
    /// Include dependency bots.
    #[arg(long)]
    pub include_bots: bool,
    /// Include drafts.
    #[arg(long)]
    pub drafts: bool,
    #[command(flatten)]
    pub review: ReviewArgs,
}

#[derive(Args, Debug)]
pub struct OpenCmdArgs {
    /// PR target.
    pub target: String,
    /// Backend override.
    #[arg(long)]
    pub backend: Option<String>,
}
