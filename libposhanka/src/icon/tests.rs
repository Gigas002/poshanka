use tempfile::tempdir;

use crate::model::IconRef;

use super::{load_icon_surface, resolve_icon_path};

fn write_test_svg(path: &std::path::Path) {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20"><rect width="10" height="20" fill="#ff0000"/></svg>"##;
    std::fs::write(path, svg).unwrap();
}

fn write_test_png(path: &std::path::Path) {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 12, 12).unwrap();
    {
        let cr = cairo::Context::new(&surface).unwrap();
        cr.set_source_rgba(1.0, 0.0, 0.0, 1.0);
        cr.paint().unwrap();
    }
    let mut file = std::fs::File::create(path).unwrap();
    surface.write_to_png(&mut file).unwrap();
}

#[test]
fn resolve_icon_path_prefers_explicit_path() {
    let dir = tempdir().unwrap();
    let icon_path = dir.path().join("bell.svg");
    write_test_svg(&icon_path);

    let icon = IconRef {
        name: Some("does-not-exist".into()),
        path: Some(icon_path.to_string_lossy().into_owned()),
    };
    let resolved = resolve_icon_path(&icon, "").expect("path should resolve");
    assert_eq!(resolved, icon_path);
}

#[test]
fn resolve_icon_path_looks_up_name_in_theme_root() {
    let dir = tempdir().unwrap();
    let apps_dir = dir.path().join("scalable/apps");
    std::fs::create_dir_all(&apps_dir).unwrap();
    let icon_path = apps_dir.join("firefox.svg");
    write_test_svg(&icon_path);

    let icon = IconRef {
        name: Some("firefox".into()),
        path: None,
    };
    let theme_root = dir.path().to_string_lossy().into_owned();
    let resolved = resolve_icon_path(&icon, &theme_root).expect("name should resolve");
    assert_eq!(resolved, icon_path);
}

#[test]
fn resolve_icon_path_returns_none_when_nothing_matches() {
    let icon = IconRef {
        name: Some("totally-unknown-icon-xyz".into()),
        path: None,
    };
    assert!(resolve_icon_path(&icon, "/nonexistent-theme-root-xyz").is_none());
}

#[test]
fn resolve_icon_path_returns_none_without_name_or_path() {
    let icon = IconRef::default();
    assert!(resolve_icon_path(&icon, "hicolor").is_none());
}

#[test]
fn load_icon_surface_rasterizes_svg_to_requested_size() {
    let dir = tempdir().unwrap();
    let icon_path = dir.path().join("bell.svg");
    write_test_svg(&icon_path);

    let surface = load_icon_surface(&icon_path, 32).expect("svg should rasterize");
    assert_eq!(surface.width(), 32);
    assert_eq!(surface.height(), 32);
}

#[test]
fn load_icon_surface_decodes_png_and_scales() {
    let dir = tempdir().unwrap();
    let icon_path = dir.path().join("bell.png");
    write_test_png(&icon_path);

    let surface = load_icon_surface(&icon_path, 32).expect("png should decode");
    assert_eq!(surface.width(), 32);
    assert_eq!(surface.height(), 32);
}
