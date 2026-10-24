//! Icon resolution and rasterization: `icon.name` / `icon.path` / raw pixel
//! data → `vello_cpu` [`Pixmap`].
//!
//! Resolution order (see [`crate::render::paint_card`]):
//! 1. `icon.raw` — decoded directly from the embedded pixel buffer (FDN
//!    `image-data` hint; e.g. chat app avatars with no theme name or file).
//! 2. `icon.path` — used directly if it points at an existing file (PNG or SVG,
//!    detected by extension).
//! 3. `icon.name` — looked up as an XDG icon-theme name under
//!    `CardStyle::icon_theme` (a theme name, or an absolute path to a theme
//!    root), falling back to the `hicolor` theme and finally the flat
//!    `/usr/share/pixmaps` directory.
//! 4. `desktop_entry` / `app_id` — when the notification carries no icon at
//!    all, or the above all fail to resolve, the notification's own app icon
//!    is looked up by that name using the same icon-theme search as step 3.
//! 5. `dialog-information` — the freedesktop icon-naming-spec generic status
//!    icon, guaranteed by any compliant theme. Only if even this fails to
//!    resolve does rendering fall back to a plain accent-colored placeholder
//!    square.
//!
//! Rendering (all results are premultiplied RGBA pixmaps):
//! - PNG is decoded by `Pixmap::from_png` and scaled to the target size with
//!   `vello_cpu` if needed.
//! - SVG is parsed and rasterized by `resvg` (pure-Rust, actively maintained)
//!   straight into a premultiplied RGBA buffer.
//! - Raw pixel data (straight, non-premultiplied alpha, row length
//!   `rowstride` which may exceed `width * channels`) is premultiplied into a
//!   pixmap, then scaled to the target size.

use std::path::{Path, PathBuf};

use std::sync::Arc;

use vello_cpu::kurbo::{Affine, Rect};
use vello_cpu::peniko::{ImageQuality, ImageSampler};
use vello_cpu::{Image, ImageSource, PixelMetadata, Pixmap, RenderContext, Resources};

use crate::error::PoshankaError;
use crate::model::{IconRef, RawIconData};

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
/// `"Adwaita"`), an absolute path to a theme root, or empty to defer to the
/// desktop's `$XDG_ICON_THEME` (falling back further to `hicolor` /
/// `/usr/share/pixmaps` if that's unset too).
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

    // An empty config value defers to the desktop's own icon theme choice,
    // same as `XDG_DATA_DIRS` below.
    let env_fallback;
    let effective_theme = if icon_theme.is_empty() {
        env_fallback = std::env::var("XDG_ICON_THEME").unwrap_or_default();
        env_fallback.as_str()
    } else {
        icon_theme
    };

    if effective_theme.starts_with('/') {
        roots.push(PathBuf::from(effective_theme));
    } else if !effective_theme.is_empty() {
        for base in data_dirs() {
            roots.push(base.join("icons").join(effective_theme));
        }
    }

    if effective_theme != "hicolor" {
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

/// Size subdirectory names to try, largest/most-scalable first. Most themes
/// follow the XDG hicolor convention (`48x48`); some (Papirus, candy-icons,
/// Sweet-Rainbow, …) use a bare number (`48`) instead.
fn size_dir_names() -> Vec<String> {
    let mut names: Vec<String> = vec!["scalable".to_string()];
    for size in SIZES {
        names.push(format!("{size}x{size}"));
        names.push(size.to_string());
    }
    names
}

fn find_in_theme_root(root: &Path, name: &str) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }

    let size_dirs = size_dir_names();

    // Try both `<size>/<context>` (XDG hicolor convention) and
    // `<context>/<size>` (Papirus-style themes, e.g. candy-icons,
    // Sweet-Rainbow) since real-world themes disagree on the order.
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

    for context in CONTEXTS {
        for size_dir in &size_dirs {
            let dir = root.join(context).join(size_dir);
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

/// Load an icon file (PNG or SVG) and rasterize it into a square
/// `size` × `size` pixmap, preserving aspect ratio and centering the result.
pub fn load_icon_pixmap(path: &Path, size: u32) -> Result<Pixmap, PoshankaError> {
    let is_svg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));

    if is_svg {
        load_svg_pixmap(path, size)
    } else {
        load_raster_pixmap(path, size)
    }
}

/// Decode raw FDN `image-data` pixel bytes (straight alpha, RGB(A), rowstride
/// possibly padded beyond `width * channels`) into a square `size` × `size`
/// pixmap, preserving aspect ratio and centering the result.
pub fn load_icon_pixmap_from_raw(raw: &RawIconData, size: u32) -> Result<Pixmap, PoshankaError> {
    if raw.width <= 0 || raw.height <= 0 {
        return Err(PoshankaError::Render("invalid raw icon dimensions".into()));
    }
    let data = base64_ng::decode(&raw.data_base64)
        .map_err(|e| PoshankaError::Render(format!("raw icon base64: {e}")))?;

    fit_to_square(raw_pixels_to_pixmap(raw, &data)?, size)
}

/// Convert straight-alpha, row-padded RGB(A) pixel bytes (FDN `image-data`
/// shape) into a premultiplied RGBA pixmap.
fn raw_pixels_to_pixmap(raw: &RawIconData, data: &[u8]) -> Result<Pixmap, PoshankaError> {
    let (width, height) = (pixmap_dim(raw.width)?, pixmap_dim(raw.height)?);
    let channels = raw.channels.max(1) as usize;
    let rowstride = raw.rowstride.max(0) as usize;
    let has_alpha = raw.has_alpha && channels >= 4;

    let row_bytes = usize::from(width) * 4;
    let mut rgba = vec![0u8; row_bytes * usize::from(height)];
    for y in 0..usize::from(height) {
        let src_row = y * rowstride;
        for x in 0..usize::from(width) {
            let src_px = src_row + x * channels;
            if src_px + channels > data.len() {
                continue; // truncated/malformed row; leave transparent
            }
            let (r, g, b) = (data[src_px], data[src_px + 1], data[src_px + 2]);
            let a = if has_alpha { data[src_px + 3] } else { 255 };
            let premultiply = |c: u8| (u16::from(c) * u16::from(a) / 255) as u8;

            let dst_px = y * row_bytes + x * 4;
            rgba[dst_px..dst_px + 4].copy_from_slice(&[
                premultiply(r),
                premultiply(g),
                premultiply(b),
                a,
            ]);
        }
    }

    Ok(Pixmap::from_parts(
        rgba,
        width,
        height,
        PixelMetadata::default(),
    ))
}

fn load_raster_pixmap(path: &Path, size: u32) -> Result<Pixmap, PoshankaError> {
    let file = std::fs::File::open(path)
        .map_err(|e| PoshankaError::Render(format!("open icon {}: {e}", path.display())))?;
    let source = Pixmap::from_png(std::io::BufReader::new(file))
        .map_err(|e| PoshankaError::Render(format!("decode icon {}: {e}", path.display())))?;
    fit_to_square(source, size)
}

fn load_svg_pixmap(path: &Path, size: u32) -> Result<Pixmap, PoshankaError> {
    let data = std::fs::read(path)
        .map_err(|e| PoshankaError::Render(format!("read icon {}: {e}", path.display())))?;

    let opts = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(&data, &opts)
        .map_err(|e| PoshankaError::Render(format!("parse svg {}: {e}", path.display())))?;

    let tree_size = tree.size();
    let (tw, th) = (tree_size.width().max(1.0), tree_size.height().max(1.0));
    let target = pixmap_dim(size.max(1))?;
    let scale = (f32::from(target) / tw).min(f32::from(target) / th);
    let dx = (f32::from(target) - tw * scale) / 2.0;
    let dy = (f32::from(target) - th * scale) / 2.0;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy);

    let mut pixmap = resvg::tiny_skia::Pixmap::new(u32::from(target), u32::from(target))
        .ok_or_else(|| PoshankaError::Render("icon pixmap allocation failed".into()))?;
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny-skia already stores premultiplied RGBA, the same layout vello_cpu uses.
    Ok(Pixmap::from_parts(
        pixmap.take(),
        target,
        target,
        PixelMetadata::default(),
    ))
}

/// Scale `source` into a new `size` × `size` pixmap, preserving aspect ratio
/// and centering. Returns `source` unchanged if it already has that size.
fn fit_to_square(source: Pixmap, size: u32) -> Result<Pixmap, PoshankaError> {
    let target = pixmap_dim(size.max(1))?;
    let (src_w, src_h) = (source.width(), source.height());
    if src_w == target && src_h == target {
        return Ok(source);
    }

    let (src_w, src_h) = (f64::from(src_w), f64::from(src_h));
    let side = f64::from(target);
    let scale = (side / src_w).min(side / src_h);
    let dx = (side - src_w * scale) / 2.0;
    let dy = (side - src_h * scale) / 2.0;

    let mut ctx = RenderContext::new(target, target);
    let mut resources = Resources::new();
    ctx.set_paint(Image {
        image: ImageSource::Pixmap(Arc::new(source)),
        sampler: ImageSampler {
            quality: ImageQuality::High,
            ..ImageSampler::default()
        },
    });
    ctx.set_paint_transform(Affine::translate((dx, dy)) * Affine::scale(scale));
    ctx.fill_rect(&Rect::new(dx, dy, dx + src_w * scale, dy + src_h * scale));

    let mut out = Pixmap::new(target, target);
    ctx.flush();
    ctx.render(&mut out, &mut resources);
    Ok(out)
}

fn pixmap_dim(value: impl TryInto<u16>) -> Result<u16, PoshankaError> {
    value
        .try_into()
        .map_err(|_| PoshankaError::Render("icon dimensions exceed renderer limits".into()))
}

#[cfg(test)]
mod tests;
