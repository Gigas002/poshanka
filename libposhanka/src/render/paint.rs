use parley::PositionedLayoutItem;
use vello_cpu::color::AlphaColor;
use vello_cpu::kurbo::{Affine, Rect, Stroke};
use vello_cpu::peniko::{BlendMode, Compose, Mix};
use vello_cpu::{Glyph, Pixmap, RenderContext, Resources};

use crate::error::PoshankaError;
use crate::model::{CardStyle, NotificationView, ProgressMode};

use super::font::{FontContext, TextLayout};
use super::markup::TextBrush;
use super::measure::{ComputedCard, measure_card};
use super::shape::rounded_rect;

/// ARGB8888 SHM buffer in **BGRA** byte order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub stride: i32,
    pub data: Vec<u8>,
}

pub fn paint_card(
    style: &CardStyle,
    notification: &NotificationView,
    font: &FontContext,
) -> Result<Frame, PoshankaError> {
    let computed = measure_card(style, notification, font)?;
    paint_computed(style, &computed, font)
}

pub fn paint_computed(
    style: &CardStyle,
    computed: &ComputedCard,
    font: &FontContext,
) -> Result<Frame, PoshankaError> {
    let width = computed.width;
    let height = computed.height.max(1);
    let (w16, h16) = match (u16::try_from(width), u16::try_from(height)) {
        (Ok(w), Ok(h)) => (w, h),
        _ => {
            return Err(PoshankaError::Render(format!(
                "card size {width}x{height} exceeds renderer limits"
            )));
        }
    };

    let mut ctx = RenderContext::new(w16, h16);
    let mut resources = Resources::new();

    let radius = f64::from(style.border_radius);
    let border = f64::from(style.border_size);
    let w = f64::from(width);
    let h = f64::from(height);

    ctx.set_paint(bgra_color(style.background_bgra));
    ctx.fill_path(&rounded_rect(0.0, 0.0, w, h, radius));

    if border > 0.0 {
        let inset = border / 2.0;
        ctx.set_paint(bgra_color(style.border_bgra));
        ctx.set_stroke(Stroke::new(border));
        let stroke_radius = (radius - inset).max(0.0);
        ctx.stroke_path(&rounded_rect(
            inset,
            inset,
            w - border,
            h - border,
            stroke_radius,
        ));
    }

    if let Some(icon) = &computed.icon {
        #[cfg(feature = "icons")]
        paint_icon(
            &mut ctx,
            style,
            icon,
            computed.icon_ref.as_ref(),
            computed.fallback_icon_name.as_deref(),
        );
        #[cfg(not(feature = "icons"))]
        paint_icon_placeholder(&mut ctx, style, icon);
    }

    if let Some(progress) = &computed.progress {
        paint_progress(&mut ctx, style, progress);
    }

    let [b, g, r, a] = style.foreground_bgra;
    let foreground = [r, g, b, a];
    for block in &computed.blocks {
        if block.height <= 0.0 {
            continue;
        }
        let layout = font.layout_markup(
            &block.markup,
            block.width,
            Some(block.height),
            &style.text_alignment,
        );
        paint_text(
            &mut ctx,
            &mut resources,
            &layout,
            block.x,
            block.y,
            foreground,
        );
    }

    let mut pixmap = Pixmap::new(w16, h16);
    ctx.flush();
    ctx.render(&mut pixmap, &mut resources);

    // vello_cpu renders premultiplied RGBA; wl_shm ARGB8888 is premultiplied
    // BGRA in memory on little-endian hosts.
    let mut data = pixmap.data_as_u8_slice().to_vec();
    let (pixels, _) = data.as_chunks_mut::<4>();
    for px in pixels {
        px.swap(0, 2);
    }

    Ok(Frame {
        width,
        height,
        stride: (width * 4) as i32,
        data,
    })
}

fn paint_text(
    ctx: &mut RenderContext,
    resources: &mut Resources,
    layout: &TextLayout,
    x: f64,
    y: f64,
    foreground: [u8; 4],
) {
    ctx.set_transform(Affine::translate((x, y)));
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let style = glyph_run.style();
            let run = glyph_run.run();

            ctx.set_paint(brush_color(style.brush, foreground));
            let coords: Vec<i16> = run
                .normalized_coords()
                .iter()
                .map(|c| c.to_bits())
                .collect();
            let mut builder = ctx
                .glyph_run(resources, run.font())
                .font_size(run.font_size())
                .normalized_coords(&coords)
                .hint(true);
            // Faux italic for families without a real italic face.
            if let Some(angle) = run.synthesis().skew() {
                builder =
                    builder.glyph_transform(Affine::skew(f64::from(angle.to_radians().tan()), 0.0));
            }
            let glyphs = glyph_run.positioned_glyphs().map(|g| Glyph {
                id: g.id,
                x: g.x,
                y: g.y,
            });
            if let Err(e) = builder.fill_glyphs(glyphs) {
                tracing::trace!(error = ?e, "some glyphs failed to render");
            }

            let metrics = run.font_metrics();
            let decorations = [
                style
                    .underline
                    .as_ref()
                    .map(|d| (d, metrics.underline_offset, metrics.underline_size)),
                style
                    .strikethrough
                    .as_ref()
                    .map(|d| (d, metrics.strikethrough_offset, metrics.strikethrough_size)),
            ];
            for (deco, offset, size) in decorations.into_iter().flatten() {
                let offset = f64::from(deco.offset.unwrap_or(offset));
                let size = f64::from(deco.size.unwrap_or(size)).max(1.0);
                let top = f64::from(glyph_run.baseline()) - offset - size / 2.0;
                let left = f64::from(glyph_run.offset());
                ctx.set_paint(brush_color(deco.brush, foreground));
                ctx.fill_rect(&Rect::new(
                    left,
                    top,
                    left + f64::from(glyph_run.advance()),
                    top + size,
                ));
            }
        }
    }
    ctx.reset_transform();
}

#[cfg(feature = "icons")]
fn paint_icon(
    ctx: &mut RenderContext,
    style: &CardStyle,
    icon: &super::measure::IconRect,
    icon_ref: Option<&crate::model::IconRef>,
    fallback_icon_name: Option<&str>,
) {
    let target_size = icon.size.round().max(1.0) as u32;

    // Raw embedded pixel data (chat app avatars, …) takes precedence — it
    // needs no filesystem lookup and is the only option those senders give us.
    if let Some(pixmap) = icon_ref
        .and_then(|r| r.raw.as_ref())
        .and_then(|raw| crate::icon::load_icon_pixmap_from_raw(raw, target_size).ok())
    {
        return paint_icon_pixmap(ctx, icon, pixmap);
    }

    if let Some(icon_ref) = icon_ref
        && let Some(path) = crate::icon::resolve_icon_path(icon_ref, &style.icon_theme)
        && let Ok(pixmap) = crate::icon::load_icon_pixmap(&path, target_size)
    {
        return paint_icon_pixmap(ctx, icon, pixmap);
    }

    if let Some(name) = fallback_icon_name
        && let Some(pixmap) = named_theme_icon(name, &style.icon_theme, target_size)
    {
        return paint_icon_pixmap(ctx, icon, pixmap);
    }

    if !style.icon_default_name.is_empty()
        && let Some(pixmap) =
            named_theme_icon(&style.icon_default_name, &style.icon_theme, target_size)
    {
        return paint_icon_pixmap(ctx, icon, pixmap);
    }

    paint_icon_placeholder(ctx, style, icon);
}

#[cfg(feature = "icons")]
fn named_theme_icon(name: &str, icon_theme: &str, target_size: u32) -> Option<Pixmap> {
    let icon_ref = crate::model::IconRef {
        name: Some(name.to_string()),
        path: None,
        raw: None,
    };
    let path = crate::icon::resolve_icon_path(&icon_ref, icon_theme)?;
    crate::icon::load_icon_pixmap(&path, target_size).ok()
}

#[cfg(feature = "icons")]
fn paint_icon_pixmap(ctx: &mut RenderContext, icon: &super::measure::IconRect, pixmap: Pixmap) {
    // Snap to whole pixels so the already size-matched icon is copied 1:1
    // instead of being resampled at a fractional offset.
    let (x, y) = (icon.x.round(), icon.y.round());
    let (w, h) = (f64::from(pixmap.width()), f64::from(pixmap.height()));
    ctx.set_paint(vello_cpu::Image {
        image: vello_cpu::ImageSource::Pixmap(std::sync::Arc::new(pixmap)),
        sampler: vello_cpu::peniko::ImageSampler::default(),
    });
    ctx.set_paint_transform(Affine::translate((x, y)));
    ctx.fill_rect(&Rect::new(x, y, x + w, y + h));
    ctx.reset_paint_transform();
}

/// Paint the filled portion of the progress bar. `style.progress_mode`
/// selects the compositing operator: `Over` blends the fill with the
/// existing card background, `Source` replaces those pixels outright
/// regardless of the fill color's alpha — matching mako's `progress-color`
/// `over`/`source` modes.
fn paint_progress(
    ctx: &mut RenderContext,
    style: &CardStyle,
    progress: &super::measure::ProgressRect,
) {
    if progress.width <= 0.0 || progress.height <= 0.0 || progress.fraction <= 0.0 {
        return;
    }

    let compose = match style.progress_mode {
        ProgressMode::Over => Compose::SrcOver,
        ProgressMode::Source => Compose::Copy,
    };
    ctx.set_blend_mode(BlendMode::new(Mix::Normal, compose));
    ctx.set_paint(bgra_color(style.progress_bgra));
    let fill_width = progress.width * progress.fraction;
    ctx.fill_rect(&Rect::new(
        progress.x,
        progress.y,
        progress.x + fill_width,
        progress.y + progress.height,
    ));
    ctx.set_blend_mode(BlendMode::default());
}

fn paint_icon_placeholder(
    ctx: &mut RenderContext,
    style: &CardStyle,
    icon: &super::measure::IconRect,
) {
    ctx.set_paint(bgra_color(style.progress_bgra));
    let r = (icon.size * 0.12).min(8.0);
    ctx.fill_path(&rounded_rect(icon.x, icon.y, icon.size, icon.size, r));
}

fn bgra_color(bgra: [u8; 4]) -> AlphaColor<vello_cpu::color::Srgb> {
    let [b, g, r, a] = bgra;
    AlphaColor::from_rgba8(r, g, b, a)
}

fn brush_color(brush: TextBrush, foreground_rgba: [u8; 4]) -> AlphaColor<vello_cpu::color::Srgb> {
    let [r, g, b, a] = brush.resolve(foreground_rgba);
    AlphaColor::from_rgba8(r, g, b, a)
}
