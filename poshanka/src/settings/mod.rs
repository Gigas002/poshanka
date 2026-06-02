#![allow(dead_code)] // Override helpers used in Phase 4+

use std::path::{Path, PathBuf};

use libposhanka::{
    CardEvents, CardStyle, DaemonSpec, IconPos, OverlaySpec, ProgressMode, TextAlign,
    parse_hex_rgba_to_bgra,
};

use crate::config::{
    Config, Events, FragmentConfig, LayerShell, OverrideType, SortBy, SortOrder, UrgencyLevel,
};
use crate::theme::{
    FragmentTheme, IconPosition, ProgressMode as TProgressMode, TextAlignment, Theme,
};

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
/// Application order: base → base_urgency → app → app_urgency
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

// ── Settings ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Settings {
    pub daemon: DaemonSpec,
    pub card: CardStyle,
}

impl Settings {
    pub fn resolve(config: &Config, theme: &Theme) -> Result<Self, crate::error::Error> {
        let daemon = build_daemon_spec(config);
        let card = build_card_style(theme, config.events.as_ref())?;
        Ok(Self { daemon, card })
    }
}

fn build_daemon_spec(config: &Config) -> DaemonSpec {
    DaemonSpec {
        stack_max: config.stack.max,
        anchor: config.placement.anchor.clone(),
        gap: config.placement.gap,
        margin: config.placement.margin,
        queue_history: config.queue.history,
        queue_max: config.queue.max,
        queue_sort: match config.queue.sort {
            SortBy::Time => "time",
            SortBy::Priority => "priority",
        }
        .into(),
        queue_order: match config.queue.order {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        }
        .into(),
        timeout_ignore: config.timeouts.ignore,
        timeout_default_ms: config.timeouts.default,
        timeout_low_ms: config.timeouts.low,
        timeout_normal_ms: config.timeouts.normal,
        timeout_critical_ms: config.timeouts.critical,
        layer: match config.layer.layer {
            LayerShell::Background => "background",
            LayerShell::Bottom => "bottom",
            LayerShell::Top => "top",
            LayerShell::Overlay => "overlay",
        }
        .into(),
        output: config.layer.output.clone(),
    }
}

fn build_card_style(
    theme: &Theme,
    events: Option<&Events>,
) -> Result<CardStyle, crate::error::Error> {
    Ok(CardStyle {
        background_bgra: parse_hex_rgba_to_bgra(&theme.colors.background)?,
        foreground_bgra: parse_hex_rgba_to_bgra(&theme.colors.foreground)?,
        border_bgra: parse_hex_rgba_to_bgra(&theme.colors.border)?,
        progress_bgra: parse_hex_rgba_to_bgra(&theme.colors.progress)?,
        font_name: theme.font.name.clone(),
        font_size: theme.font.size,
        width: theme.layout.width,
        height: theme.layout.height,
        padding: theme.layout.padding,
        margin: theme.layout.margin,
        border_size: theme.border.size,
        border_radius: theme.border.radius,
        text_alignment: match theme.text.alignment {
            TextAlignment::Left => TextAlign::Left,
            TextAlignment::Center => TextAlign::Center,
            TextAlignment::Right => TextAlign::Right,
        },
        summary_template: theme.text.summary.clone(),
        body_template: theme.text.body.clone(),
        app_template: theme.text.app.clone(),
        id_template: theme.text.id.clone(),
        icon_size: theme.icons.size,
        icon_position: match theme.icons.position {
            IconPosition::Left => IconPos::Left,
            IconPosition::Right => IconPos::Right,
            IconPosition::Top => IconPos::Top,
            IconPosition::Bottom => IconPos::Bottom,
        },
        icon_theme: theme.icons.theme.clone(),
        progress_mode: match theme.progress.mode {
            TProgressMode::Over => ProgressMode::Over,
            TProgressMode::Source => ProgressMode::Source,
        },
        events: events
            .map(|e| CardEvents {
                on_button_left: e.on_button_left.clone(),
                on_button_middle: e.on_button_middle.clone(),
                on_button_right: e.on_button_right.clone(),
                on_notify: e.on_notify.clone(),
                on_touch: e.on_touch.clone(),
            })
            .unwrap_or_default(),
    })
}

/// Build a `CardStyle` from a merged (post-override) theme and resolved events.
///
/// Used at notification time in Phase 4 after `apply_layers` + `resolve_events`.
pub fn card_style_from_theme(
    theme: &Theme,
    events: Option<&Events>,
) -> Result<CardStyle, crate::error::Error> {
    build_card_style(theme, events)
}

/// Derive a Phase 0 overlay spec from a `CardStyle` (backward compat until Phase 3).
pub fn overlay_spec_from_card(card: &CardStyle) -> OverlaySpec {
    OverlaySpec::new(card.width, card.height, card.background_bgra)
}

#[cfg(test)]
mod tests;
