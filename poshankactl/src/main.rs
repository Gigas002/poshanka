use std::process::ExitCode;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "poshankactl",
    about = "Control client for the poshanka notification daemon"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Check that the daemon is running
    Ping,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "poshankactl=info".into()),
        )
        .init();

    let _cli = Cli::parse();
    tracing::error!("poshankactl: not yet implemented");
    ExitCode::from(1)
}
