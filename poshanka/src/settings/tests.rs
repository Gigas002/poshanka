use std::path::Path;

use super::Settings;
use crate::config::Config;
use crate::theme::Theme;

fn examples_dir() -> std::path::PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples")).to_path_buf()
}

#[test]
fn resolve_builds_overlay_from_examples() {
    let config = Config::load(&examples_dir().join("config.toml")).unwrap();
    let theme = Theme::load(&examples_dir().join("theme.toml")).unwrap();
    let settings = Settings::resolve(&config, &theme).unwrap();
    // background #285577ff → BGRA: [0x77, 0x55, 0x28, 0xff]
    assert_eq!(settings.overlay.background_bgra, [0x77, 0x55, 0x28, 0xff]);
}

#[test]
fn resolve_rejects_invalid_hex_color() {
    let config = Config::load(&examples_dir().join("config.toml")).unwrap();
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
