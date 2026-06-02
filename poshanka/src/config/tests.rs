use std::path::Path;

use super::{
    Config, Events, FragmentConfig, OverrideType, SortBy, SortOrder, UrgencyLevel, config_dir,
    config_dir_from_env, default_config_path,
};

// ── main config ──────────────────────────────────────────────────────────────

#[test]
fn deserializes_examples_config() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/config.toml"
    ));
    let cfg = Config::load(path).expect("examples/config.toml must parse");
    assert_eq!(cfg.paths.theme, "theme.toml");
    assert_eq!(cfg.paths.overrides.len(), 3);
    assert_eq!(cfg.stack.max, 5);
    assert_eq!(cfg.placement.anchor, "bottom-right");
    assert_eq!(cfg.placement.gap, 10);
    assert_eq!(cfg.placement.margin, 0);
    let _ = cfg.queue.history; // confirms field exists
    assert_eq!(cfg.queue.sort, SortBy::Time);
    assert_eq!(cfg.queue.order, SortOrder::Desc);
    assert!(!cfg.timeouts.ignore);
    assert_eq!(cfg.timeouts.default, 0);
    assert_eq!(cfg.timeouts.low, 5000);
    assert_eq!(cfg.timeouts.normal, 10000);
    assert_eq!(cfg.timeouts.critical, 0);
}

#[test]
fn config_events_defaults_to_none() {
    let raw = r#"
[paths]
theme = "theme.toml"

[stack]
max = 3

[placement]
anchor = "top-right"
gap = 5
margin = 0

[queue]
history = false
max = 10
sort = "time"
order = "asc"

[timeouts]
ignore = false
default = 0
low = 5000
normal = 10000
critical = 0

[layer]
layer = "overlay"
output = ""
"#;
    let cfg: Config = toml::from_str(raw).unwrap();
    assert!(cfg.events.is_none());
}

#[test]
fn config_events_parses_shell_hooks() {
    let raw = r#"
[paths]
theme = "theme.toml"

[stack]
max = 3

[placement]
anchor = "top-right"
gap = 5
margin = 0

[queue]
history = false
max = 10
sort = "time"
order = "asc"

[timeouts]
ignore = false
default = 0
low = 5000
normal = 10000
critical = 0

[layer]
layer = "overlay"
output = ""

[events]
on_button_left = "wmctrl -a Firefox"
"#;
    let cfg: Config = toml::from_str(raw).unwrap();
    let events = cfg.events.as_ref().unwrap();
    assert_eq!(events.on_button_left.as_deref(), Some("wmctrl -a Firefox"));
    assert!(events.on_button_middle.is_none());
}

#[test]
fn load_missing_file_errors() {
    let err = Config::load(Path::new("/nonexistent/poshanka/config.toml")).unwrap_err();
    assert!(matches!(err, crate::error::Error::Io { .. }));
}

// ── fragment configs ──────────────────────────────────────────────────────────

#[test]
fn deserializes_urgency_low_fragment() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/urgency/low/config.toml"
    ));
    let frag = FragmentConfig::load(path).expect("urgency/low/config.toml must parse");
    assert_eq!(frag.override_meta.kind, OverrideType::Urgency);
    assert_eq!(frag.override_meta.level, Some(UrgencyLevel::Low));
    assert!(frag.override_meta.name.is_none());
    assert_eq!(
        frag.paths.as_ref().and_then(|p| p.theme.as_deref()),
        Some("theme.toml")
    );
    assert!(frag.events.is_none());
}

#[test]
fn deserializes_urgency_critical_fragment() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/urgency/critical/config.toml"
    ));
    let frag = FragmentConfig::load(path).expect("urgency/critical/config.toml must parse");
    assert_eq!(frag.override_meta.kind, OverrideType::Urgency);
    assert_eq!(frag.override_meta.level, Some(UrgencyLevel::Critical));
}

#[test]
fn deserializes_app_fragment() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/apps/some_app/config.toml"
    ));
    let frag = FragmentConfig::load(path).expect("apps/some_app/config.toml must parse");
    assert_eq!(frag.override_meta.kind, OverrideType::App);
    assert_eq!(frag.override_meta.name.as_deref(), Some("some_app"));
    assert!(frag.override_meta.level.is_none());
    let paths = frag.paths.as_ref().unwrap();
    assert_eq!(paths.overrides.len(), 2);
}

#[test]
fn deserializes_app_urgency_nested_fragments() {
    for (suffix, expected_level) in [
        ("urgency/low/config.toml", UrgencyLevel::Low),
        ("urgency/critical/config.toml", UrgencyLevel::Critical),
    ] {
        let path_str = format!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/apps/some_app/{}"),
            suffix
        );
        let path = Path::new(&path_str);
        let frag = FragmentConfig::load(path)
            .unwrap_or_else(|e| panic!("{} must parse: {e}", path.display()));
        assert_eq!(frag.override_meta.kind, OverrideType::Urgency);
        assert_eq!(frag.override_meta.level, Some(expected_level));
    }
}

// ── path helpers (XDG resolution) ────────────────────────────────────────────

#[test]
fn config_dir_ends_with_poshanka() {
    assert!(config_dir().ends_with("poshanka"));
}

#[test]
fn default_config_path_ends_with_config_toml() {
    assert!(default_config_path().ends_with("poshanka/config.toml"));
}

#[test]
fn xdg_config_home_used_when_set() {
    let dir = config_dir_from_env(Some("/xdg/config"), Some("/home/user"));
    assert_eq!(dir, std::path::Path::new("/xdg/config/poshanka"));
}

#[test]
fn empty_xdg_config_home_falls_back_to_home() {
    let dir = config_dir_from_env(Some(""), Some("/home/user"));
    assert_eq!(dir, std::path::Path::new("/home/user/.config/poshanka"));
}

#[test]
fn no_xdg_config_home_uses_home_dot_config() {
    let dir = config_dir_from_env(None::<&str>, Some("/home/user"));
    assert_eq!(dir, std::path::Path::new("/home/user/.config/poshanka"));
}

#[test]
fn no_xdg_no_home_falls_back_to_dot_config() {
    let dir = config_dir_from_env(None::<&str>, None::<&str>);
    assert_eq!(dir, std::path::Path::new(".config/poshanka"));
}

// ── round-trip of Events ──────────────────────────────────────────────────────

#[test]
fn events_all_none_by_default() {
    let e: Events = toml::from_str("").unwrap();
    assert!(e.on_button_left.is_none());
    assert!(e.on_button_middle.is_none());
    assert!(e.on_button_right.is_none());
    assert!(e.on_notify.is_none());
    assert!(e.on_touch.is_none());
}
