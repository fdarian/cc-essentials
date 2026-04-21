mod cli;
mod detect;
mod fs_util;

fn main() -> anyhow::Result<()> {
    cli::run()
}
