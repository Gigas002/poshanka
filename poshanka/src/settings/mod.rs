use libposhanka::{OverlaySpec, parse_hex_rgba_to_bgra};

use crate::config::Config;
use crate::theme::Theme;

/// Phase 0 placeholder size for the solid overlay (future: driven by layout/theme).
const OVERLAY_WIDTH: u32 = 320;
const OVERLAY_HEIGHT: u32 = 120;

#[derive(Debug, Clone)]
pub struct Settings {
    pub overlay: OverlaySpec,
}

impl Settings {
    pub fn resolve(config: &Config, theme: &Theme) -> Result<Self, crate::error::Error> {
        let _ = config; // will be used in phase 1 step 4 (DaemonSpec)
        let background = parse_hex_rgba_to_bgra(&theme.colors.background)?;
        Ok(Self {
            overlay: OverlaySpec::new(OVERLAY_WIDTH, OVERLAY_HEIGHT, background),
        })
    }
}

#[cfg(test)]
mod tests;
