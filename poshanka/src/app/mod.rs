use std::path::Path;

use libposhanka::{ProviderSpec, SubscriberRun, run_subscriber};

use crate::settings::{Settings, overlay_spec_from_card};
use crate::theme;

pub fn run(settings: &Settings, config_path: &Path) -> std::process::ExitCode {
    let mut provider = ProviderSpec::from(&settings.subscriber);
    if let Some(exec) = provider.exec.as_ref() {
        provider.exec = Some(
            theme::resolve_path(config_path, exec)
                .to_string_lossy()
                .into_owned(),
        );
    }

    let run = SubscriberRun {
        provider,
        overlay: overlay_spec_from_card(&settings.card),
    };

    match run_subscriber(run) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(%err, "subscriber session ended with an error");
            std::process::ExitCode::from(1)
        }
    }
}
