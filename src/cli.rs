use crate::cache;
use crate::commands;
use anyhow::Context;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cc-essentials", version, about = "Claude Code essentials")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Health-check detection (formatter, repo root, cache, etc.)
    Doctor,
    /// Show error dumps and recent log entries.
    Logs,
    /// Claude Code hooks.
    Hooks {
        #[command(subcommand)]
        cmd: HooksCommand,
    },
}

#[derive(Subcommand)]
enum HooksCommand {
    /// Check and write: format the file and report diagnostics.
    Crite,
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => {
            let start = std::env::current_dir().context("cwd")?;
            let cache = cache::Cache::open()?;
            let use_color = std::io::IsTerminal::is_terminal(&std::io::stdout());
            let mut out = std::io::stdout().lock();
            commands::doctor::run(&start, &cache, &mut out, use_color)?;
        }
        Command::Logs => {
            let cache = cache::Cache::open()?;
            let use_color = std::io::IsTerminal::is_terminal(&std::io::stdout());
            let mut out = std::io::stdout().lock();
            commands::logs::run(&cache, &mut out, use_color)?;
        }
        Command::Hooks { cmd } => match cmd {
            HooksCommand::Crite => {
                let cache = cache::Cache::open()?;
                let mut stdin = std::io::stdin().lock();
                let mut stdout = std::io::stdout().lock();
                commands::hooks_crite::run(&cache, &mut stdin, &mut stdout)?;
            }
        },
    }
    Ok(())
}
