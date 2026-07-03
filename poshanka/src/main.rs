mod app;
mod cli;
mod config;
mod error;
mod logger;
mod settings;
mod theme;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;
use crate::config::{Config, default_config_path};
use crate::settings::Settings;
use crate::theme::Theme;

fn main() -> ExitCode {
    logger::init();
    let cli = Cli::parse();

    let config_path = cli.config.clone().unwrap_or_else(default_config_path);
    let config = match Config::load(&config_path) {
        Ok(c) => c,
        Err(err) => {
            tracing::error!(%err, path = %config_path.display(), "failed to load config");
            return ExitCode::from(1);
        }
    };

    let theme_path = theme::resolve_path(&config_path, &config.paths.theme);
    let theme = match Theme::load(&theme_path) {
        Ok(t) => t,
        Err(err) => {
            tracing::error!(%err, path = %theme_path.display(), "failed to load theme");
            return ExitCode::from(1);
        }
    };

    let settings = match Settings::resolve(&config, &theme) {
        Ok(s) => s,
        Err(err) => {
            tracing::error!(%err, "failed to resolve settings");
            return ExitCode::from(1);
        }
    };

    tracing::info!(
        font = %settings.card.font_name,
        font_size = settings.card.font_size,
        anchor = %settings.subscriber.anchor,
        layer = %settings.subscriber.layer,
        provider_exec = ?settings.subscriber.exec,
        provider_command = ?settings.subscriber.command,
        theme = %theme_path.display(),
        "poshanka starting"
    );

    app::run(&settings)
}
