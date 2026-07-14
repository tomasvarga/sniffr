//! sniffr — Rust port (in progress). The bash `bin/sniffr` remains the reference
//! on `main`; this branch ports it to a proper binary. Implemented so far:
//! CLI surface, config/finding types, `doctor`, `version`. Ports pending:
//! review pipeline (agent run → resolve → inject), consensus, queue, setup, update.
mod cli;
mod config;
mod doctor;
mod finding;

use clap::Parser;
use cli::{Cli, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Version) => println!("sniffr {}", env!("CARGO_PKG_VERSION")),
        Some(Command::Doctor) => doctor::run()?,
        Some(Command::Setup) => not_yet("setup"),
        Some(Command::Update) => not_yet("update"),
        Some(Command::Queue(_)) => not_yet("queue"),
        Some(Command::OpenCmd(_)) => not_yet("open-cmd"),
        None => match cli.target {
            // TODO: review::run(target, cli.review, Config::load()).await?
            Some(target) => not_yet(&format!("review {target}")),
            None => {
                // no target, no subcommand → show help
                Cli::parse_from(["sniffr", "--help"]);
            }
        },
    }
    Ok(())
}

fn not_yet(what: &str) {
    eprintln!("sniffr: `{what}` isn't ported to the Rust build yet — use the bash `bin/sniffr` on `main`.");
    std::process::exit(2);
}
