use std::path::Path;

use super::{Settings, apply_layers, load_overrides, resolve_events, resolve_layers};
use crate::config::{Config, OverrideType, UrgencyLevel};
use crate::theme::Theme;

fn examples_dir() -> std::path::PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples")).to_path_buf()
}

fn load_examples() -> (Config, std::path::PathBuf, Theme) {
    let dir = examples_dir();
    let config_path = dir.join("config.toml");
    let config = Config::load(&config_path).unwrap();
    let theme = Theme::load(&dir.join("theme.toml")).unwrap();
    (config, config_path, theme)
}

// ── Settings::resolve (Phase 0) ───────────────────────────────────────────────

#[test]
fn resolve_builds_overlay_from_examples() {
    let (config, _, theme) = load_examples();
    let settings = Settings::resolve(&config, &theme).unwrap();
    // background #285577ff → BGRA: [0x77, 0x55, 0x28, 0xff]
    assert_eq!(settings.overlay.background_bgra, [0x77, 0x55, 0x28, 0xff]);
}

#[test]
fn resolve_rejects_invalid_hex_color() {
    let (config, _, _) = load_examples();
    let raw_theme = r##"
[font]
name = "Noto Sans"
size = 14.0

[colors]
background = "not-a-color"
foreground = "#ffffffff"
border = "#4c7899ff"
progress = "#5588aaff"

[layout]
width = 300
height = 100
padding = 5
margin = 10

[border]
size = 2
radius = 0

[text]
alignment = "left"
summary = "<b>{summary}</b>"
body = "{body}"

[icons]
size = 64
position = "left"
theme = ""

[progress]
mode = "over"
"##;
    let theme: Theme = toml::from_str(raw_theme).unwrap();
    assert!(Settings::resolve(&config, &theme).is_err());
}

// ── load_overrides ────────────────────────────────────────────────────────────

#[test]
fn load_overrides_returns_all_top_level_fragments() {
    let (config, config_path, _) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();
    // examples/config.toml has 3 overrides: urgency/low, urgency/critical, apps/some_app
    assert_eq!(overrides.len(), 3);
}

#[test]
fn load_overrides_urgency_fragments_have_themes() {
    let (config, config_path, _) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();
    for ov in &overrides {
        if ov.config.override_meta.kind == OverrideType::Urgency {
            assert!(
                ov.theme.is_some(),
                "urgency fragment {:?} should have a theme",
                ov.config.override_meta.level
            );
        }
    }
}

#[test]
fn load_overrides_app_fragment_has_nested_overrides() {
    let (config, config_path, _) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();
    let app = overrides
        .iter()
        .find(|o| o.config.override_meta.kind == OverrideType::App)
        .expect("apps/some_app must be loaded");
    assert_eq!(app.nested.len(), 2);
    assert!(
        app.nested
            .iter()
            .all(|n| n.config.override_meta.kind == OverrideType::Urgency)
    );
}

// ── resolve_layers ────────────────────────────────────────────────────────────

#[test]
fn urgency_only_populates_base_urgency() {
    let (config, config_path, _) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();

    let layers = resolve_layers(&overrides, None, Some(&UrgencyLevel::Critical));
    assert!(layers.base_urgency.is_some());
    assert_eq!(
        layers.base_urgency.unwrap().config.override_meta.level,
        Some(UrgencyLevel::Critical)
    );
    assert!(layers.app.is_none());
    assert!(layers.app_urgency.is_none());
}

#[test]
fn app_only_populates_app_layer() {
    let (config, config_path, _) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();

    let layers = resolve_layers(&overrides, Some("some_app"), None);
    assert!(layers.base_urgency.is_none());
    assert_eq!(
        layers.app.unwrap().config.override_meta.name.as_deref(),
        Some("some_app")
    );
    assert!(layers.app_urgency.is_none());
}

#[test]
fn app_and_urgency_populates_all_three_layers() {
    let (config, config_path, _) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();

    // some_app + Critical: all three layers should be populated
    let layers = resolve_layers(&overrides, Some("some_app"), Some(&UrgencyLevel::Critical));
    assert!(
        layers.base_urgency.is_some(),
        "global urgency/critical should match"
    );
    assert!(layers.app.is_some(), "apps/some_app should match");
    assert!(
        layers.app_urgency.is_some(),
        "apps/some_app/urgency/critical should match"
    );
}

#[test]
fn app_with_unmatched_urgency_has_no_app_urgency() {
    let (config, config_path, _) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();

    // apps/some_app has no urgency/normal sub-override
    let layers = resolve_layers(&overrides, Some("some_app"), Some(&UrgencyLevel::Normal));
    assert!(layers.base_urgency.is_none());
    assert!(layers.app.is_some());
    assert!(layers.app_urgency.is_none());
}

#[test]
fn unknown_app_all_layers_none() {
    let (config, config_path, _) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();

    let layers = resolve_layers(&overrides, Some("unknown_app"), Some(&UrgencyLevel::Normal));
    assert!(layers.base_urgency.is_none());
    assert!(layers.app.is_none());
    assert!(layers.app_urgency.is_none());
}

#[test]
fn no_context_all_layers_none() {
    let (config, config_path, _) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();

    let layers = resolve_layers(&overrides, None, None);
    assert!(layers.base_urgency.is_none());
    assert!(layers.app.is_none());
    assert!(layers.app_urgency.is_none());
}

// ── apply_layers ──────────────────────────────────────────────────────────────

#[test]
fn urgency_low_colors_applied() {
    let (config, config_path, base) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();

    let layers = resolve_layers(&overrides, None, Some(&UrgencyLevel::Low));
    let merged = apply_layers(&base, &layers);
    assert_eq!(merged.colors.background, "#2e3440ff");
    assert_eq!(merged.colors.border, "#4c566aff");
    // keys absent in urgency/low fragment keep base values
    assert_eq!(merged.colors.foreground, "#ffffffff");
    assert_eq!(merged.colors.progress, "#5588aaff");
    // non-color sections unchanged
    assert_eq!(merged.font.name, "Noto Sans");
    assert_eq!(merged.layout.width, 300);
}

#[test]
fn urgency_critical_colors_applied() {
    let (config, config_path, base) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();

    let layers = resolve_layers(&overrides, None, Some(&UrgencyLevel::Critical));
    let merged = apply_layers(&base, &layers);
    assert_eq!(merged.colors.background, "#bf616aff");
    assert_eq!(merged.colors.border, "#d08770ff");
}

#[test]
fn app_urgency_layers_stack_in_specificity_order() {
    // some_app + Critical:
    //   base theme       → background = #285577ff
    //   base urgency     → background = #bf616aff (critical red)
    //   app              → background = #285577ff (app resets to its base color)
    //   app urgency      → background = #bf616aff (most specific: critical within app)
    let (config, config_path, base) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();

    let layers = resolve_layers(&overrides, Some("some_app"), Some(&UrgencyLevel::Critical));
    assert!(layers.base_urgency.is_some());
    assert!(layers.app.is_some());
    assert!(layers.app_urgency.is_some());

    let merged = apply_layers(&base, &layers);
    // app urgency (most specific) wins
    assert_eq!(merged.colors.background, "#bf616aff");
    assert_eq!(merged.colors.border, "#d08770ff");
}

#[test]
fn no_layers_returns_base_unchanged() {
    let (config, config_path, base) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();

    let layers = resolve_layers(&overrides, None, None);
    let merged = apply_layers(&base, &layers);
    assert_eq!(merged.colors.background, base.colors.background);
    assert_eq!(merged.font.name, base.font.name);
}

// ── resolve_events ────────────────────────────────────────────────────────────

#[test]
fn resolve_events_falls_back_to_base() {
    let (config, config_path, _) = load_examples();
    let overrides = load_overrides(&config, &config_path).unwrap();
    // urgency/low fragment has no [events]; base config has an empty [events] table
    let layers = resolve_layers(&overrides, None, Some(&UrgencyLevel::Low));
    assert!(layers.base_urgency.unwrap().config.events.is_none());
    let events = resolve_events(config.events.as_ref(), &layers);
    // falls back to base; examples/config.toml [events] table exists but all keys absent
    let ev = events.expect("base [events] table is present");
    assert!(ev.on_button_left.is_none());
    assert!(ev.on_button_right.is_none());
}
