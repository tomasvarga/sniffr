//! `sniffr setup` — print the agent setup prompt (embedded at compile time).
const SETUP_PROMPT: &str = include_str!("../../SETUP_PROMPT.md");

pub fn run() {
    print!("{SETUP_PROMPT}");
}
