use libposhanka::{OverlaySpec, parse_hex_rgba_to_bgra};

use crate::config::Config;
use crate::theme::Theme;

/// Phase 0 placeholder size for the solid overlay (future: driven by layout/theme).
const OVERLAY_WIDTH: u32 = 320;
const OVERLAY_HEIGHT: u32 = 120;

#[derive(Debug, Clone)]
pub struct Settings {
    pub overlay: OverlaySpec,
    pub font_name: String,
    pub font_size: f64,
}

impl Settings {
    pub fn resolve(config: &Config, theme: &Theme) -> Result<Self, crate::error::Error> {
        if config.base.font_name.trim().is_empty() {
            return Err(crate::error::Error::MissingBaseField("font_name"));
        }

        let font_size = config.base.font_size.unwrap_or(14.0);
        let background = parse_hex_rgba_to_bgra(&theme.base.background_color)?;

        Ok(Self {
            overlay: OverlaySpec::new(OVERLAY_WIDTH, OVERLAY_HEIGHT, background),
            font_name: config.base.font_name.clone(),
            font_size,
        })
    }
}

#[cfg(test)]
mod tests;
