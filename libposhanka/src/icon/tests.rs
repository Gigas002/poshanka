use tempfile::tempdir;

use crate::model::{IconRef, RawIconData};

use super::{load_icon_surface, load_icon_surface_from_raw, resolve_icon_path};

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
        raw: None,
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
        raw: None,
    };
    let theme_root = dir.path().to_string_lossy().into_owned();
    let resolved = resolve_icon_path(&icon, &theme_root).expect("name should resolve");
    assert_eq!(resolved, icon_path);
}

#[test]
fn resolve_icon_path_looks_up_name_in_context_first_theme_root() {
    // Papirus-style layout (candy-icons, Sweet-Rainbow, …): `<context>/<size>`
    // instead of the XDG hicolor `<size>/<context>` convention.
    let dir = tempdir().unwrap();
    let apps_dir = dir.path().join("apps/scalable");
    std::fs::create_dir_all(&apps_dir).unwrap();
    let icon_path = apps_dir.join("firefox.svg");
    write_test_svg(&icon_path);

    let icon = IconRef {
        name: Some("firefox".into()),
        path: None,
        raw: None,
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
        raw: None,
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

fn raw_icon(width: i32, height: i32, has_alpha: bool, pixels: &[u8]) -> RawIconData {
    let channels = if has_alpha { 4 } else { 3 };
    RawIconData {
        width,
        height,
        rowstride: width * channels,
        has_alpha,
        bits_per_sample: 8,
        channels,
        data_base64: base64_ng::encode(pixels).unwrap(),
    }
}

#[test]
fn load_icon_surface_from_raw_decodes_and_scales() {
    // 2x1 opaque red RGBA.
    let raw = raw_icon(2, 1, true, &[255, 0, 0, 255, 255, 0, 0, 255]);
    let surface = load_icon_surface_from_raw(&raw, 8).expect("raw icon should decode");
    assert_eq!(surface.width(), 8);
    assert_eq!(surface.height(), 8);
}

#[test]
fn load_icon_surface_from_raw_premultiplies_alpha() {
    // Single half-transparent red pixel: straight R=200, A=128.
    let raw = raw_icon(1, 1, true, &[200, 0, 0, 128]);
    let mut surface = load_icon_surface_from_raw(&raw, 1).expect("raw icon should decode");
    let data = surface.data().unwrap();
    // BGRA order; premultiplied red ~= 200 * 128 / 255 = 100.
    let px = &data[0..4];
    assert_eq!(px[3], 128, "alpha byte unchanged");
    assert!(
        (95..=105).contains(&px[2]),
        "red channel should be premultiplied by alpha, got {}",
        px[2]
    );
}

#[test]
fn load_icon_surface_from_raw_respects_rowstride_padding() {
    // 2x2 opaque RGB (3 channels) with 2 bytes of row padding beyond
    // width*channels on each row. Target size matches native size exactly so
    // no scaling/interpolation is involved — pixel values must be exact.
    let mut raw = raw_icon(2, 2, false, &[]);
    raw.rowstride = 2 * 3 + 2;
    #[rustfmt::skip]
    let pixels = [
        0, 255, 0,   0, 0, 255,   0xAA, 0xAA, // row0: green, blue, padding
        255, 0, 0,   255, 255, 0, 0xAA, 0xAA, // row1: red, yellow, padding
    ];
    raw.data_base64 = base64_ng::encode(&pixels).unwrap();

    let mut surface = load_icon_surface_from_raw(&raw, 2).expect("raw icon should decode");
    assert_eq!((surface.width(), surface.height()), (2, 2));
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    // Pixel (x=1, y=0) in the destination BGRA surface (4 bytes/px there
    // regardless of the 3-channel source) — should be blue, not padding
    // bytes misread as a pixel.
    let px1 = &data[4..8];
    assert_eq!((px1[0], px1[1], px1[2], px1[3]), (255, 0, 0, 255));
    // Pixel (x=0, y=1) — confirms the second row is read starting at
    // `rowstride`, not `width * channels`.
    let px2 = &data[stride..stride + 4];
    assert_eq!((px2[0], px2[1], px2[2], px2[3]), (0, 0, 255, 255));
}

#[test]
fn load_icon_surface_from_raw_rejects_invalid_dimensions() {
    let raw = raw_icon(0, 0, true, &[]);
    assert!(load_icon_surface_from_raw(&raw, 8).is_err());
}
