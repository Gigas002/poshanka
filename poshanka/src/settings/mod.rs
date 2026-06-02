#![allow(dead_code)] // Phase 1 step 2: used by step 4 (Settings::resolve → DaemonSpec + CardStyle)

use std::path::{Path, PathBuf};

use libposhanka::{OverlaySpec, parse_hex_rgba_to_bgra};

use crate::config::{Config, Events, FragmentConfig, OverrideType, UrgencyLevel};
use crate::theme::{FragmentTheme, Theme};

// ── Override loading ──────────────────────────────────────────────────────────

/// A loaded override fragment: its parsed config, optional associated theme, and
/// any nested sub-fragments (populated only for `app`-type fragments).
pub struct LoadedOverride {
    pub config: FragmentConfig,
    /// Theme loaded from `config.paths.theme`, resolved relative to the fragment directory.
    pub theme: Option<FragmentTheme>,
    /// Nested sub-overrides from `config.paths.overrides` (app-type only).
    pub nested: Vec<LoadedOverride>,
}

/// Load all override fragments listed in `config.paths.overrides`.
///
/// Each path is resolved relative to `config_path`'s parent directory.
/// For `app`-type fragments, nested overrides in their own `[paths].overrides`
/// are also loaded (recursively).
pub fn load_overrides(
    config: &Config,
    config_path: &Path,
) -> Result<Vec<LoadedOverride>, crate::error::Error> {
    let dir = config_path.parent().unwrap_or(Path::new(""));
    config
        .paths
        .overrides
        .iter()
        .map(|rel| load_single_override(&dir.join(rel)))
        .collect()
}

fn load_single_override(fragment_path: &Path) -> Result<LoadedOverride, crate::error::Error> {
    let config = FragmentConfig::load(fragment_path)?;
    let dir = fragment_path.parent().unwrap_or(Path::new(""));

    let theme = config
        .paths
        .as_ref()
        .and_then(|p| p.theme.as_deref())
        .map(|name| {
            let theme_path = if Path::new(name).is_absolute() {
                PathBuf::from(name)
            } else {
                dir.join(name)
            };
            FragmentTheme::load(&theme_path)
        })
        .transpose()?;

    // Nested overrides are relative to the fragment's own directory.
    let nested = config
        .paths
        .as_ref()
        .map(|p| p.overrides.as_slice())
        .unwrap_or(&[])
        .iter()
        .map(|rel| load_single_override(&dir.join(rel)))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(LoadedOverride {
        config,
        theme,
        nested,
    })
}

// ── Override resolution ───────────────────────────────────────────────────────

/// All applicable override layers for a notification context, in application order.
///
/// Precedence (highest last, so later layers win):
/// base theme/config → base urgency → app → app urgency
pub struct OverrideLayers<'a> {
    /// Global urgency-type override matching the notification's urgency level (if any).
    pub base_urgency: Option<&'a LoadedOverride>,
    /// App-type override matching the notification's `app_name` (if any).
    pub app: Option<&'a LoadedOverride>,
    /// Urgency sub-override inside `app`, matching the notification's urgency (if any).
    pub app_urgency: Option<&'a LoadedOverride>,
}

/// Resolve all applicable override layers for the given notification context.
///
/// Unlike a "first wins" scan, this finds urgency and app overrides **independently**
/// and returns them all so [`apply_layers`] can stack them in specificity order.
pub fn resolve_layers<'a>(
    overrides: &'a [LoadedOverride],
    app_name: Option<&str>,
    urgency: Option<&UrgencyLevel>,
) -> OverrideLayers<'a> {
    let base_urgency = urgency.and_then(|u| {
        overrides.iter().find(|ov| {
            ov.config.override_meta.kind == OverrideType::Urgency
                && ov.config.override_meta.level.as_ref() == Some(u)
        })
    });

    let app_ov = app_name.and_then(|name| {
        overrides.iter().find(|ov| {
            ov.config.override_meta.kind == OverrideType::App
                && ov.config.override_meta.name.as_deref() == Some(name)
        })
    });

    let app_urgency = app_ov.and_then(|app| {
        urgency.and_then(|u| {
            app.nested.iter().find(|sub| {
                sub.config.override_meta.kind == OverrideType::Urgency
                    && sub.config.override_meta.level.as_ref() == Some(u)
            })
        })
    });

    OverrideLayers {
        base_urgency,
        app: app_ov,
        app_urgency,
    }
}

// ── Override application ──────────────────────────────────────────────────────

/// Apply all override layers to `base`, returning the merged theme.
///
/// Application order (each layer patches the previous):
/// base → base_urgency → app → app_urgency
pub fn apply_layers(base: &Theme, layers: &OverrideLayers<'_>) -> Theme {
    let t = layers
        .base_urgency
        .and_then(|ov| ov.theme.as_ref())
        .map(|f| base.apply_fragment(f))
        .unwrap_or_else(|| base.clone());

    let t = layers
        .app
        .and_then(|ov| ov.theme.as_ref())
        .map(|f| t.apply_fragment(f))
        .unwrap_or(t);

    layers
        .app_urgency
        .and_then(|ov| ov.theme.as_ref())
        .map(|f| t.apply_fragment(f))
        .unwrap_or(t)
}

/// Resolve the effective `[events]` for a notification context.
///
/// Most-specific wins: app_urgency > app > base_urgency > base config events.
pub fn resolve_events<'a>(
    base: Option<&'a Events>,
    layers: &OverrideLayers<'a>,
) -> Option<&'a Events> {
    layers
        .app_urgency
        .and_then(|o| o.config.events.as_ref())
        .or_else(|| layers.app.and_then(|o| o.config.events.as_ref()))
        .or_else(|| layers.base_urgency.and_then(|o| o.config.events.as_ref()))
        .or(base)
}

// ── Settings (Phase 0 overlay placeholder) ────────────────────────────────────

/// Phase 0 placeholder size; driven by layout/theme in Phase 1 step 4.
const OVERLAY_WIDTH: u32 = 320;
const OVERLAY_HEIGHT: u32 = 120;

#[derive(Debug, Clone)]
pub struct Settings {
    pub overlay: OverlaySpec,
}

impl Settings {
    pub fn resolve(config: &Config, theme: &Theme) -> Result<Self, crate::error::Error> {
        let _ = config;
        let background = parse_hex_rgba_to_bgra(&theme.colors.background)?;
        Ok(Self {
            overlay: OverlaySpec::new(OVERLAY_WIDTH, OVERLAY_HEIGHT, background),
        })
    }
}

#[cfg(test)]
mod tests;
