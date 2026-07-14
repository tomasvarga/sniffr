//! sniffr — Rust port (in progress). The bash `bin/sniffr` on `main` is the
//! reference until this reaches parity. Ported: CLI, config/finding types,
//! `doctor`, `version`, and the `--format json` review path (agent → resolve →
//! filter). Pending: inject backends, consensus, `--bg`, queue, setup, update.
mod agent;
mod cli;
mod config;
mod doctor;
mod finding;
mod prompt;
mod resolve;
mod review;
mod target;

use clap::Parser;
use cli::{Cli, Command};
use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Version) => println!("sniffr {}", env!("CARGO_PKG_VERSION")),
        Some(Command::Doctor) => doctor::run()?,
        Some(Command::Setup) => not_yet("setup"),
        Some(Command::Update) => not_yet("update"),
        Some(Command::Queue(_)) => not_yet("queue"),
        Some(Command::OpenCmd(_)) => not_yet("open-cmd"),
        None => match &cli.target {
            Some(t) => review::run(t, &cli.review, &Config::load()).await?,
            None => {
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
