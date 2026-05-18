use super::Settings;
use crate::config::{Base, Config};
use crate::theme::{Theme, ThemeBase};

#[test]
fn resolve_builds_overlay_from_theme() {
    let config = Config {
        base: Base {
            font_name: "Noto Sans".into(),
            font_size: Some(12.0),
            theme: None,
        },
    };
    let theme = Theme {
        base: ThemeBase {
            background_color: "#ff0000ff".into(),
            foreground_color: None,
        },
    };
    let settings = Settings::resolve(&config, &theme).unwrap();
    assert_eq!(settings.overlay.background_bgra, [0, 0, 255, 255]);
    assert_eq!(settings.font_name, "Noto Sans");
}

#[test]
fn rejects_empty_font_name() {
    let config = Config {
        base: Base {
            font_name: "   ".into(),
            font_size: None,
            theme: None,
        },
    };
    let theme = Theme {
        base: ThemeBase {
            background_color: "#000000ff".into(),
            foreground_color: None,
        },
    };
    assert!(Settings::resolve(&config, &theme).is_err());
}
