use cairo::{Context, Format, ImageSurface, Operator};
use pangocairo::functions::show_layout;

use crate::error::PoshankaError;
use crate::model::{CardStyle, NotificationView};

use super::font::FontContext;
use super::measure::{ComputedCard, measure_card, text_align_to_pango};
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
    let stride = width
        .checked_mul(4)
        .ok_or_else(|| PoshankaError::Render("stride overflow".into()))? as i32;

    let surface = ImageSurface::create(Format::ARgb32, width as i32, height as i32)
        .map_err(|e| PoshankaError::Render(format!("cairo surface: {e}")))?;

    {
        let cr = Context::new(&surface)
            .map_err(|e| PoshankaError::Render(format!("cairo context: {e}")))?;

        cr.set_operator(Operator::Clear);
        cr.paint()
            .map_err(|e| PoshankaError::Render(format!("clear: {e}")))?;
        cr.set_operator(Operator::Over);

        let radius = f64::from(style.border_radius);
        let border = f64::from(style.border_size);
        let w = f64::from(width);
        let h = f64::from(height);

        set_source_bgra(&cr, style.background_bgra);
        rounded_rect(&cr, 0.0, 0.0, w, h, radius);
        cr.fill()
            .map_err(|e| PoshankaError::Render(format!("background fill: {e}")))?;

        if border > 0.0 {
            let inset = border / 2.0;
            set_source_bgra(&cr, style.border_bgra);
            cr.set_line_width(border);
            let stroke_radius = (radius - inset).max(0.0);
            rounded_rect(&cr, inset, inset, w - border, h - border, stroke_radius);
            cr.stroke()
                .map_err(|e| PoshankaError::Render(format!("border stroke: {e}")))?;
        }

        if let Some(icon) = &computed.icon {
            paint_icon_placeholder(&cr, style, icon)?;
        }

        font.set_alignment(text_align_to_pango(&style.text_alignment));
        for block in &computed.blocks {
            if block.height <= 0.0 {
                continue;
            }
            font.layout().set_markup(&block.markup);
            font.layout()
                .set_width((block.width * f64::from(pango::SCALE)).round() as i32);
            font.layout().set_wrap(pango::WrapMode::WordChar);
            font.layout()
                .set_height((block.height * f64::from(pango::SCALE)).round() as i32);
            font.layout().set_ellipsize(pango::EllipsizeMode::End);

            cr.move_to(block.x, block.y);
            set_source_bgra(&cr, style.foreground_bgra);
            show_layout(&cr, font.layout());
        }
    }

    let mut data = surface
        .take_data()
        .map_err(|e| PoshankaError::Render(format!("take_data: {e}")))?
        .to_vec();

    let row_bytes = (width * 4) as usize;
    if data.len() >= (stride * height as i32) as usize && stride as usize != row_bytes {
        let mut tight = vec![0u8; row_bytes * height as usize];
        for row in 0..height as usize {
            let src = row * stride as usize;
            let dst = row * row_bytes;
            tight[dst..dst + row_bytes].copy_from_slice(&data[src..src + row_bytes]);
        }
        data = tight;
    }

    Ok(Frame {
        width,
        height,
        stride: row_bytes as i32,
        data,
    })
}

fn paint_icon_placeholder(
    cr: &Context,
    style: &CardStyle,
    icon: &super::measure::IconRect,
) -> Result<(), PoshankaError> {
    set_source_bgra(cr, style.progress_bgra);
    let r = (icon.size * 0.12).min(8.0);
    rounded_rect(cr, icon.x, icon.y, icon.size, icon.size, r);
    cr.fill()
        .map_err(|e| PoshankaError::Render(format!("icon placeholder: {e}")))?;
    Ok(())
}

fn set_source_bgra(cr: &Context, bgra: [u8; 4]) {
    let [b, g, r, a] = bgra;
    cr.set_source_rgba(
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
        f64::from(a) / 255.0,
    );
}
