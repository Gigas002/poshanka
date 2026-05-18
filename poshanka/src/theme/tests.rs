use std::path::Path;

use libposhanka::parse_hex_rgba_to_bgra;

use super::{Theme, resolve_path};

#[test]
fn deserializes_base_colors() {
    let raw = r##"
[base]
background_color = "#1e1e2eff"
foreground_color = "#cdd6f4ff"
"##;
    let theme: Theme = toml::from_str(raw).unwrap();
    assert_eq!(theme.base.background_color, "#1e1e2eff");
    parse_hex_rgba_to_bgra(&theme.base.background_color).unwrap();
}

#[test]
fn resolve_relative_beside_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "[base]\nfont_name = \"A\"\n").unwrap();
    let theme = dir.path().join("theme.toml");
    std::fs::write(&theme, "[base]\nbackground_color = \"#000000ff\"\n").unwrap();
    assert_eq!(resolve_path(&config, "theme.toml"), theme);
}

#[test]
fn load_missing_theme_errors() {
    let err = Theme::load(Path::new("/nonexistent/poshanka/theme.toml")).unwrap_err();
    assert!(matches!(err, crate::error::Error::Io { .. }));
}
