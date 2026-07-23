use std::path::{Path, PathBuf};

use libposhanka::{CardStyle, NotificationView, StyleSource, Urgency};

use crate::config::{Config, UrgencyLevel};
use crate::settings::{
    LoadedOverride, apply_layers, card_style_from_theme, load_overrides, resolve_layers,
};
use crate::theme::{self, Theme};

/// [`StyleSource`] backed by poshanka's TOML config/theme + override fragments.
///
/// Resolves a per-notification [`CardStyle`] by merging app/urgency override
/// fragments over the base theme (see [`resolve_layers`] / [`apply_layers`]).
/// `reload()` re-reads config, theme, and override fragments from disk after a
/// provider `reload` event, falling back to the previous state on failure.
pub struct OverrideStyleSource {
    config_path: PathBuf,
    base_theme: Theme,
    overrides: Vec<LoadedOverride>,
    default_style: CardStyle,
}

impl OverrideStyleSource {
    /// Load config, theme, and override fragments from disk.
    pub fn load(config_path: &Path) -> Result<Self, crate::error::Error> {
        let config = Config::load(config_path)?;
        let theme_path = theme::resolve_path(config_path, &config.paths.theme);
        let base_theme = Theme::load(&theme_path)?;
        let overrides = load_overrides(&config, config_path)?;
        let default_style = card_style_from_theme(&base_theme)?;
        Ok(Self {
            config_path: config_path.to_path_buf(),
            base_theme,
            overrides,
            default_style,
        })
    }
}

impl StyleSource for OverrideStyleSource {
    fn style_for(&mut self, notification: &NotificationView) -> CardStyle {
        let urgency = to_urgency_level(notification.urgency);
        let layers = resolve_layers(&self.overrides, Some(&notification.app_id), Some(&urgency));
        let merged = apply_layers(&self.base_theme, &layers);
        card_style_from_theme(&merged).unwrap_or_else(|err| {
            tracing::warn!(
                %err,
                app_id = %notification.app_id,
                "override theme produced an invalid style; using base theme"
            );
            self.default_style.clone()
        })
    }

    fn reload(&mut self) {
        match Self::load(&self.config_path) {
            Ok(fresh) => {
                *self = fresh;
                tracing::info!(path = %self.config_path.display(), "reloaded poshanka config/theme");
            }
            Err(err) => {
                tracing::warn!(%err, "failed to reload config/theme; keeping previous settings");
            }
        }
    }
}

fn to_urgency_level(urgency: Urgency) -> UrgencyLevel {
    match urgency {
        Urgency::Low => UrgencyLevel::Low,
        Urgency::Normal => UrgencyLevel::Normal,
        Urgency::Critical => UrgencyLevel::Critical,
    }
}
