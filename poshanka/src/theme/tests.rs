use std::path::Path;

use super::{FragmentTheme, IconPosition, ProgressMode, TextAlignment, Theme, resolve_path};

// ── main theme ────────────────────────────────────────────────────────────────

#[test]
fn deserializes_examples_theme() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/theme.toml"
    ));
    let theme = Theme::load(path).expect("examples/theme.toml must parse");
    assert_eq!(theme.font.name, "Noto Sans");
    assert!((theme.font.size - 14.0).abs() < f64::EPSILON);
    assert_eq!(theme.colors.background, "#285577ff");
    assert_eq!(theme.colors.foreground, "#ffffffff");
    assert_eq!(theme.colors.border, "#4c7899ff");
    assert_eq!(theme.colors.progress, "#5588aaff");
    assert_eq!(theme.layout.width, 300);
    assert_eq!(theme.layout.height, 100);
    assert_eq!(theme.layout.padding, 5);
    assert_eq!(theme.layout.margin, 10);
    assert_eq!(theme.border.size, 2);
    assert_eq!(theme.border.radius, 0);
    assert_eq!(theme.text.alignment, TextAlignment::Left);
    assert_eq!(theme.text.summary, "<b>{summary}</b>");
    assert_eq!(theme.text.body, "{body}");
    assert!(theme.text.app.is_none());
    assert!(theme.text.id.is_none());
    assert_eq!(theme.icons.size, 64);
    assert_eq!(theme.icons.position, IconPosition::Left);
    assert_eq!(theme.icons.theme, "");
    assert_eq!(theme.progress.mode, ProgressMode::Over);
}

#[test]
fn load_missing_theme_errors() {
    let err = Theme::load(Path::new("/nonexistent/poshanka/theme.toml")).unwrap_err();
    assert!(matches!(err, crate::error::Error::Io { .. }));
}

// ── fragment themes ───────────────────────────────────────────────────────────

#[test]
fn deserializes_urgency_low_fragment_theme() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/urgency/low/theme.toml"
    ));
    let frag = FragmentTheme::load(path).expect("urgency/low/theme.toml must parse");
    let colors = frag.colors.as_ref().unwrap();
    assert_eq!(colors.background.as_deref(), Some("#2e3440ff"));
    assert_eq!(colors.border.as_deref(), Some("#4c566aff"));
    assert!(colors.foreground.is_none());
    assert!(colors.progress.is_none());
    assert!(frag.font.is_none());
}

#[test]
fn deserializes_urgency_critical_fragment_theme() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/urgency/critical/theme.toml"
    ));
    let frag = FragmentTheme::load(path).expect("urgency/critical/theme.toml must parse");
    let colors = frag.colors.as_ref().unwrap();
    assert_eq!(colors.background.as_deref(), Some("#bf616aff"));
    assert_eq!(colors.border.as_deref(), Some("#d08770ff"));
}

#[test]
fn deserializes_app_full_fragment_theme() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/apps/some_app/theme.toml"
    ));
    let frag = FragmentTheme::load(path).expect("apps/some_app/theme.toml must parse");
    let font = frag.font.as_ref().unwrap();
    assert_eq!(font.name, "Noto Sans");
    let colors = frag.colors.as_ref().unwrap();
    assert_eq!(colors.background.as_deref(), Some("#285577ff"));
}

#[test]
fn deserializes_app_nested_urgency_fragment_themes() {
    for suffix in ["urgency/low/theme.toml", "urgency/critical/theme.toml"] {
        let path_str = format!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/apps/some_app/{}"),
            suffix
        );
        let path = Path::new(&path_str);
        FragmentTheme::load(path).unwrap_or_else(|e| panic!("{} must parse: {e}", path.display()));
    }
}

// ── apply_fragment ────────────────────────────────────────────────────────────

#[test]
fn apply_fragment_replaces_color_keys_present_in_fragment() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/theme.toml"
    ));
    let base = Theme::load(path).unwrap();

    let frag = FragmentTheme {
        colors: Some(super::FragmentColors {
            background: Some("#2e3440ff".into()),
            border: Some("#4c566aff".into()),
            foreground: None,
            progress: None,
        }),
        ..Default::default()
    };

    let merged = base.apply_fragment(&frag);
    assert_eq!(merged.colors.background, "#2e3440ff");
    assert_eq!(merged.colors.border, "#4c566aff");
    // keys absent in frag keep base values
    assert_eq!(merged.colors.foreground, "#ffffffff");
    assert_eq!(merged.colors.progress, "#5588aaff");
    // non-color sections unchanged
    assert_eq!(merged.font.name, "Noto Sans");
}

#[test]
fn apply_fragment_with_no_overrides_is_identity() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/theme.toml"
    ));
    let base = Theme::load(path).unwrap();
    let merged = base.apply_fragment(&FragmentTheme::default());
    assert_eq!(merged.colors.background, base.colors.background);
    assert_eq!(merged.font.name, base.font.name);
    assert_eq!(merged.layout.width, base.layout.width);
}

#[test]
fn apply_fragment_replaces_whole_section_when_present() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/theme.toml"
    ));
    let base = Theme::load(path).unwrap();

    let frag = FragmentTheme {
        font: Some(super::Font {
            name: "Monospace".into(),
            size: 12.0,
        }),
        ..Default::default()
    };

    let merged = base.apply_fragment(&frag);
    assert_eq!(merged.font.name, "Monospace");
    assert!((merged.font.size - 12.0).abs() < f64::EPSILON);
    // other sections unchanged
    assert_eq!(merged.colors.background, base.colors.background);
}

// ── path resolution ───────────────────────────────────────────────────────────

#[test]
fn resolve_absolute_path_unchanged() {
    let result = resolve_path(Path::new("/any/config.toml"), "/abs/theme.toml");
    assert_eq!(result, Path::new("/abs/theme.toml"));
}

#[test]
fn resolve_relative_beside_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "").unwrap();
    let theme = dir.path().join("theme.toml");
    std::fs::write(&theme, "").unwrap();
    assert_eq!(resolve_path(&config, "theme.toml"), theme);
}

#[test]
fn resolve_falls_back_to_themes_dir_when_not_beside() {
    let result = resolve_path(Path::new("/no/such/config.toml"), "my-theme.toml");
    assert!(result.ends_with("poshanka/themes/my-theme.toml"));
}
