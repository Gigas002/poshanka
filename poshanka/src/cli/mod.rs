use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "poshanka", about = "Minimal Wayland notification daemon")]
pub struct Cli {
    /// Path to config.toml (default: XDG …/poshanka/config.toml)
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Path to theme.toml (default: resolved from config `base.theme`)
    #[arg(long, value_name = "PATH")]
    pub theme: Option<PathBuf>,
}

#[cfg(test)]
mod tests;
