mod style;

use std::path::Path;

use libposhanka::{ProviderSpec, SubscriberRun, run_subscriber};

use crate::settings::Settings;
use crate::theme;
use style::OverrideStyleSource;

pub fn run(settings: &Settings, config_path: &Path) -> std::process::ExitCode {
    let mut provider = ProviderSpec::from(&settings.subscriber);
    if let Some(exec) = provider.exec.as_ref() {
        provider.exec = Some(
            theme::resolve_path(config_path, exec)
                .to_string_lossy()
                .into_owned(),
        );
    }

    let style_source = match OverrideStyleSource::load(config_path) {
        Ok(source) => source,
        Err(err) => {
            tracing::error!(%err, path = %config_path.display(), "failed to load override style source");
            return std::process::ExitCode::from(1);
        }
    };

    let run = SubscriberRun {
        provider,
        stack: settings.subscriber.clone(),
        style_source: Box::new(style_source),
    };

    match run_subscriber(run) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(%err, "subscriber session ended with an error");
            std::process::ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests;
