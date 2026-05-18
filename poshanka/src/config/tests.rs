use std::path::Path;

use super::{Config, config_dir, default_config_path};

#[test]
fn deserializes_minimal_base() {
    let raw = r##"
[base]
font_name = "Noto Sans"
font_size = 14.0
theme = "theme.toml"
"##;
    let cfg: Config = toml::from_str(raw).unwrap();
    assert_eq!(cfg.base.font_name, "Noto Sans");
    assert_eq!(cfg.base.font_size, Some(14.0));
    assert_eq!(cfg.base.theme.as_deref(), Some("theme.toml"));
}

#[test]
fn config_dir_ends_with_poshanka() {
    assert!(config_dir().ends_with("poshanka"));
}

#[test]
fn default_config_path_ends_with_config_toml() {
    assert!(default_config_path().ends_with("poshanka/config.toml"));
}

#[test]
fn load_missing_file_errors() {
    let err = Config::load(Path::new("/nonexistent/poshanka/config.toml")).unwrap_err();
    assert!(matches!(err, crate::error::Error::Io { .. }));
}
