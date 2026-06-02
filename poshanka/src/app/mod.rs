use std::process::ExitCode;

use crate::settings::{Settings, overlay_spec_from_card};

pub fn run(settings: &Settings) -> ExitCode {
    let overlay = overlay_spec_from_card(&settings.card);
    match libposhanka::run_overlay(overlay) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(%err, "Wayland session ended with an error");
            ExitCode::from(1)
        }
    }
}
