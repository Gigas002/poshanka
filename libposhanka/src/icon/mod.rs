//! Icon resolution and rasterization: `icon.name` / `icon.path` → Cairo surface.
//!
//! Resolution order:
//! 1. `icon.path` — used directly if it points at an existing file (PNG or SVG,
//!    detected by extension).
//! 2. `icon.name` — looked up as an XDG icon-theme name under
//!    `CardStyle::icon_theme` (a theme name, or an absolute path to a theme
//!    root), falling back to the `hicolor` theme and finally the flat
//!    `/usr/share/pixmaps` directory.
//!
//! Rendering:
//! - PNG is decoded directly by `cairo::ImageSurface::create_from_png` and
//!   scaled to the target size if needed.
//! - SVG is parsed and rasterized by `resvg` (pure-Rust, actively maintained)
//!   into a premultiplied RGBA buffer, then copied into a Cairo `ImageSurface`.

use std::path::{Path, PathBuf};

use cairo::{Context, Format, ImageSurface};

use crate::error::PoshankaError;
use crate::model::IconRef;

/// Icon-theme subdirectory "contexts" searched, in priority order, per size.
const CONTEXTS: [&str; 6] = [
    "apps",
    "status",
    "categories",
    "devices",
    "mimetypes",
    "places",
];

/// Raster icon sizes searched, largest first (downscaling looks better than
/// upscaling).
const SIZES: [u32; 7] = [256, 128, 96, 64, 48, 32, 24];

/// Resolve an [`IconRef`] to a concrete icon file path on disk.
///
/// `icon_theme` is `CardStyle::icon_theme`: an XDG icon theme name (e.g.
/// `"Adwaita"`), an absolute path to a theme root, or empty to disable
/// name-based lookups (only `icon.path` will resolve).
pub fn resolve_icon_path(icon: &IconRef, icon_theme: &str) -> Option<PathBuf> {
    if let Some(path) = &icon.path {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let name = icon.name.as_deref()?;
    if name.is_empty() {
        return None;
    }

    let as_path = Path::new(name);
    if as_path.is_absolute() && as_path.is_file() {
        return Some(as_path.to_path_buf());
    }

    for root in theme_roots(icon_theme) {
        if let Some(found) = find_in_theme_root(&root, name) {
            return Some(found);
        }
    }

    for ext in ["png", "svg", "xpm"] {
        let candidate = PathBuf::from(format!("/usr/share/pixmaps/{name}.{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn theme_roots(icon_theme: &str) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if icon_theme.starts_with('/') {
        roots.push(PathBuf::from(icon_theme));
    } else if !icon_theme.is_empty() {
        for base in data_dirs() {
            roots.push(base.join("icons").join(icon_theme));
        }
    }

    if icon_theme != "hicolor" {
        for base in data_dirs() {
            roots.push(base.join("icons").join("hicolor"));
        }
    }

    roots
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(&home).join(".local/share"));
        dirs.push(PathBuf::from(&home).join(".icons"));
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_DIRS") {
        dirs.extend(xdg.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    } else {
        dirs.push(PathBuf::from("/usr/local/share"));
        dirs.push(PathBuf::from("/usr/share"));
    }
    dirs
}

fn find_in_theme_root(root: &Path, name: &str) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }

    let mut size_dirs: Vec<String> = vec!["scalable".to_string()];
    size_dirs.extend(SIZES.iter().map(|s| format!("{s}x{s}")));

    for size_dir in &size_dirs {
        for context in CONTEXTS {
            let dir = root.join(size_dir).join(context);
            for ext in ["svg", "png"] {
                let candidate = dir.join(format!("{name}.{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

/// Load an icon file (PNG or SVG) and rasterize it into a square Cairo
/// `ImageSurface` of `size` × `size` pixels, preserving aspect ratio and
/// centering the result.
pub fn load_icon_surface(path: &Path, size: u32) -> Result<ImageSurface, PoshankaError> {
    let is_svg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));

    if is_svg {
        load_svg_surface(path, size)
    } else {
        load_raster_surface(path, size)
    }
}

fn load_raster_surface(path: &Path, size: u32) -> Result<ImageSurface, PoshankaError> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| PoshankaError::Render(format!("open icon {}: {e}", path.display())))?;
    let source = ImageSurface::create_from_png(&mut file)
        .map_err(|e| PoshankaError::Render(format!("decode icon {}: {e}", path.display())))?;

    let (src_w, src_h) = (source.width(), source.height());
    if src_w == size as i32 && src_h == size as i32 {
        return Ok(source);
    }
    scale_surface(&source, src_w, src_h, size)
}

fn load_svg_surface(path: &Path, size: u32) -> Result<ImageSurface, PoshankaError> {
    let data = std::fs::read(path)
        .map_err(|e| PoshankaError::Render(format!("read icon {}: {e}", path.display())))?;

    let opts = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(&data, &opts)
        .map_err(|e| PoshankaError::Render(format!("parse svg {}: {e}", path.display())))?;

    let tree_size = tree.size();
    let (tw, th) = (tree_size.width().max(1.0), tree_size.height().max(1.0));
    let target = size.max(1);
    let scale = (target as f32 / tw).min(target as f32 / th);
    let dx = (target as f32 - tw * scale) / 2.0;
    let dy = (target as f32 - th * scale) / 2.0;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy);

    let mut pixmap = resvg::tiny_skia::Pixmap::new(target, target)
        .ok_or_else(|| PoshankaError::Render("icon pixmap allocation failed".into()))?;
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    rgba_to_argb_surface(pixmap.data(), target, target)
}

/// Scale an existing (PNG-decoded) `ImageSurface` into a new `size` × `size`
/// surface, preserving aspect ratio and centering.
fn scale_surface(
    source: &ImageSurface,
    src_w: i32,
    src_h: i32,
    size: u32,
) -> Result<ImageSurface, PoshankaError> {
    let target = ImageSurface::create(Format::ARgb32, size as i32, size as i32)
        .map_err(|e| PoshankaError::Render(format!("icon surface: {e}")))?;
    let cr =
        Context::new(&target).map_err(|e| PoshankaError::Render(format!("icon context: {e}")))?;

    let scale = (f64::from(size) / f64::from(src_w)).min(f64::from(size) / f64::from(src_h));
    let dx = (f64::from(size) - f64::from(src_w) * scale) / 2.0;
    let dy = (f64::from(size) - f64::from(src_h) * scale) / 2.0;

    cr.translate(dx, dy);
    cr.scale(scale, scale);
    cr.set_source_surface(source, 0.0, 0.0)
        .map_err(|e| PoshankaError::Render(format!("icon source: {e}")))?;
    cr.paint()
        .map_err(|e| PoshankaError::Render(format!("icon paint: {e}")))?;

    Ok(target)
}

/// Convert a premultiplied RGBA (tiny-skia, byte order R,G,B,A) buffer into a
/// Cairo `Format::ARgb32` surface (premultiplied, native-endian 32-bit ARGB,
/// i.e. B,G,R,A byte order on little-endian hosts).
fn rgba_to_argb_surface(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<ImageSurface, PoshankaError> {
    let mut surface = ImageSurface::create(Format::ARgb32, width as i32, height as i32)
        .map_err(|e| PoshankaError::Render(format!("icon surface: {e}")))?;
    let stride = surface.stride();
    let row_bytes = (width * 4) as usize;

    {
        let mut data = surface
            .data()
            .map_err(|e| PoshankaError::Render(format!("icon surface data: {e}")))?;
        for row in 0..height as usize {
            let src = &rgba[row * row_bytes..row * row_bytes + row_bytes];
            let dst_start = row * stride as usize;
            let dst = &mut data[dst_start..dst_start + row_bytes];
            for (px_src, px_dst) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
                let (r, g, b, a) = (px_src[0], px_src[1], px_src[2], px_src[3]);
                px_dst[0] = b;
                px_dst[1] = g;
                px_dst[2] = r;
                px_dst[3] = a;
            }
        }
    }

    Ok(surface)
}

#[cfg(test)]
mod tests;
