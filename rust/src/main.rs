//! sniffr — an AI sniffs your PR for issues before you review it. Rust port of
//! the bash `bin/sniffr` (agent → line-anchored findings → tuicr/hunk/custom or
//! JSON; multi-agent consensus via a cheap model).
mod agent;
mod backend;
mod cli;
mod config;
mod consensus;
mod doctor;
mod finding;
mod prompt;
mod queue;
mod resolve;
mod review;
mod setup;
mod target;
mod update;

use clap::{CommandFactory, Parser};
use cli::{Cli, Command};
use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Version) => println!("sniffr {}", env!("CARGO_PKG_VERSION")),
        Some(Command::Doctor) => doctor::run()?,
        Some(Command::Setup) => setup::run(),
        Some(Command::Update) => update::run()?,
        Some(Command::Queue(q)) => queue::run(q, &Config::load()).await?,
        Some(Command::OpenCmd(o)) => {
            let cfg = Config::load();
            let tgt = target::resolve(Some(&o.target), false, false)?;
            let diff = target::diff(&tgt).await?;
            let backend_name = backend::resolve(&cfg, o.backend.as_deref());
            let patch = std::env::temp_dir()
                .join(format!("sniffr-{}.patch", tgt.num))
                .to_string_lossy()
                .into_owned();
            if let Some(cmd) = backend::open_command(&backend_name, &tgt, &patch, &diff, &cfg).await? {
                println!("{cmd}");
            }
        }
        None => {
            // review is the default action — for a PR/range/stdin target, or a
            // local diff (--diff/--staged) with no positional target.
            if cli.target.is_some() || cli.review.diff || cli.review.staged {
                review::run(cli.target.as_deref(), &cli.review, &Config::load()).await?;
            } else {
                Cli::command().print_help()?;
                println!();
            }
        }
    }
    Ok(())
}
