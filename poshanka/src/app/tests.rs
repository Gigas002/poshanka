use std::path::{Path, PathBuf};

use libposhanka::{NotificationView, StyleSource, Urgency};

use super::style::OverrideStyleSource;

fn sample_view(app_id: &str, urgency: Urgency) -> NotificationView {
    NotificationView {
        id: 1,
        app_id: app_id.into(),
        summary: "Hello".into(),
        body: "World".into(),
        urgency,
        timeout_ms: Some(5_000),
        has_actions: false,
    }
}

fn copy_dir_all(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let dest_path = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_all(&entry.path(), &dest_path);
        } else {
            std::fs::copy(entry.path(), &dest_path).unwrap();
        }
    }
}

/// Copy `examples/` into a fresh tempdir so tests can mutate config/theme
/// files on disk (e.g. to exercise `reload()`) without touching the real
/// `examples/` fixtures.
fn temp_examples() -> (tempfile::TempDir, PathBuf) {
    let examples = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples"));
    let dir = tempfile::tempdir().unwrap();
    copy_dir_all(examples, dir.path());
    let config_path = dir.path().join("config.toml");
    (dir, config_path)
}

#[test]
fn style_for_unmatched_app_and_urgency_uses_base_theme() {
    let (_dir, config_path) = temp_examples();
    let mut source = OverrideStyleSource::load(&config_path).unwrap();
    let style = source.style_for(&sample_view("unknown_app", Urgency::Normal));
    assert_eq!(style.background_bgra, [0x77, 0x55, 0x28, 0xff]); // #285577ff
}

#[test]
fn style_for_urgency_override_applies() {
    let (_dir, config_path) = temp_examples();
    let mut source = OverrideStyleSource::load(&config_path).unwrap();
    let style = source.style_for(&sample_view("unknown_app", Urgency::Critical));
    assert_eq!(style.background_bgra[2], 0xbf); // R byte of #bf616aff
}

#[test]
fn style_for_app_and_urgency_layers_stack_in_specificity_order() {
    let (_dir, config_path) = temp_examples();
    let mut source = OverrideStyleSource::load(&config_path).unwrap();
    let style = source.style_for(&sample_view("some_app", Urgency::Critical));
    // Most specific override (app + urgency) wins, matching settings::apply_layers tests.
    assert_eq!(style.background_bgra[2], 0xbf);
}

#[test]
fn reload_picks_up_theme_changes_on_disk() {
    let (_dir, config_path) = temp_examples();
    let mut source = OverrideStyleSource::load(&config_path).unwrap();

    let theme_path = config_path.parent().unwrap().join("theme.toml");
    let updated = std::fs::read_to_string(&theme_path)
        .unwrap()
        .replace("#285577ff", "#112233ff");
    std::fs::write(&theme_path, updated).unwrap();

    source.reload();

    let style = source.style_for(&sample_view("unknown_app", Urgency::Normal));
    assert_eq!(style.background_bgra, [0x33, 0x22, 0x11, 0xff]);
}

#[test]
fn reload_keeps_previous_style_when_disk_config_is_broken() {
    let (_dir, config_path) = temp_examples();
    let mut source = OverrideStyleSource::load(&config_path).unwrap();

    std::fs::write(&config_path, "not valid toml").unwrap();
    source.reload();

    // Falls back to the last good style rather than panicking or losing state.
    let style = source.style_for(&sample_view("unknown_app", Urgency::Normal));
    assert_eq!(style.background_bgra, [0x77, 0x55, 0x28, 0xff]);
}
