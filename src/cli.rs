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
            println!("doctor: not yet implemented");
        }
        Command::Hooks { cmd } => match cmd {
            HooksCommand::Crite => {
                println!("hooks crite: not yet implemented");
            }
        },
    }
    Ok(())
}
