use crate::error::PoshankaError;
use crate::model::{CardStyle, IconPos, NotificationView, TextAlign};

use super::font::FontContext;
use super::template::apply_template;

const TEXT_GAP: f64 = 4.0;
const ICON_GAP: f64 = 4.0;

#[derive(Debug, Clone, PartialEq)]
pub struct IconRect {
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextBlock {
    pub markup: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedCard {
    pub width: u32,
    pub height: u32,
    pub blocks: Vec<TextBlock>,
    pub icon: Option<IconRect>,
}

pub fn measure_card(
    style: &CardStyle,
    notification: &NotificationView,
    font: &FontContext,
) -> Result<ComputedCard, PoshankaError> {
    let width = style.width.max(1);
    let max_height = style.height.max(1) as f64;
    let border = f64::from(style.border_size);
    let padding = f64::from(style.padding);
    let inner_w = f64::from(width) - 2.0 * (border + padding);
    let inner_h = max_height - 2.0 * (border + padding);

    let icon_size = f64::from(style.icon_size.max(0));
    let reserve_icon = icon_size > 0.0;

    let (text_x, text_y, text_w, text_h, icon) = text_region(
        style,
        inner_w,
        inner_h,
        icon_size,
        reserve_icon,
        border + padding,
        border + padding,
    );

    let mut blocks = Vec::new();
    let mut y = text_y;
    let mut used_h = 0.0;

    if let Some(tpl) = &style.app_template {
        let markup = apply_template(tpl, notification);
        let remaining = (text_h - used_h).max(0.0);
        let (_, block_h) = font.measure_markup(&markup, text_w, Some(remaining));
        blocks.push(TextBlock {
            markup,
            x: text_x,
            y,
            width: text_w,
            height: block_h,
        });
        used_h += block_h;
        y += block_h + TEXT_GAP;
    }

    let summary_markup = apply_template(&style.summary_template, notification);
    let summary_remaining = (text_h - used_h).max(0.0);
    let (_, summary_h) = font.measure_markup(&summary_markup, text_w, Some(summary_remaining));
    blocks.push(TextBlock {
        markup: summary_markup,
        x: text_x,
        y,
        width: text_w,
        height: summary_h,
    });
    used_h += summary_h;
    y += summary_h + TEXT_GAP;

    let body_markup = apply_template(&style.body_template, notification);
    let body_remaining = (text_h - used_h).max(0.0);
    let (_, body_h) = font.measure_markup(&body_markup, text_w, Some(body_remaining));
    blocks.push(TextBlock {
        markup: body_markup,
        x: text_x,
        y,
        width: text_w,
        height: body_h,
    });
    used_h += body_h;
    y += body_h;

    if let Some(tpl) = &style.id_template
        && !tpl.is_empty()
    {
        let markup = apply_template(tpl, notification);
        let remaining = (text_h - used_h).max(0.0);
        if remaining > 0.0 {
            y += TEXT_GAP;
            let (_, block_h) = font.measure_markup(&markup, text_w, Some(remaining));
            blocks.push(TextBlock {
                markup,
                x: text_x,
                y,
                width: text_w,
                height: block_h,
            });
            used_h += TEXT_GAP + block_h;
        }
    }

    let text_stack_h = used_h;
    let content_h = match style.icon_position {
        IconPos::Top | IconPos::Bottom if reserve_icon => icon_size + ICON_GAP + text_stack_h,
        _ if reserve_icon => text_stack_h.max(icon_size),
        _ => text_stack_h,
    };

    let natural_h = (2.0 * (border + padding) + content_h).round() as u32;
    let height = natural_h.min(style.height).max(1);

    Ok(ComputedCard {
        width,
        height,
        blocks,
        icon,
    })
}

fn text_region(
    style: &CardStyle,
    inner_w: f64,
    inner_h: f64,
    icon_size: f64,
    reserve_icon: bool,
    origin_x: f64,
    origin_y: f64,
) -> (f64, f64, f64, f64, Option<IconRect>) {
    if !reserve_icon {
        return (origin_x, origin_y, inner_w.max(1.0), inner_h.max(1.0), None);
    }

    match style.icon_position {
        IconPos::Left => {
            let text_w = (inner_w - icon_size - ICON_GAP).max(1.0);
            let icon = IconRect {
                x: origin_x,
                y: origin_y + (inner_h - icon_size) / 2.0,
                size: icon_size,
            };
            (
                origin_x + icon_size + ICON_GAP,
                origin_y,
                text_w,
                inner_h,
                Some(icon),
            )
        }
        IconPos::Right => {
            let text_w = (inner_w - icon_size - ICON_GAP).max(1.0);
            let icon = IconRect {
                x: origin_x + text_w + ICON_GAP,
                y: origin_y + (inner_h - icon_size) / 2.0,
                size: icon_size,
            };
            (origin_x, origin_y, text_w, inner_h, Some(icon))
        }
        IconPos::Top => {
            let text_h = (inner_h - icon_size - ICON_GAP).max(1.0);
            let icon = IconRect {
                x: origin_x + (inner_w - icon_size) / 2.0,
                y: origin_y,
                size: icon_size,
            };
            (
                origin_x,
                origin_y + icon_size + ICON_GAP,
                inner_w,
                text_h,
                Some(icon),
            )
        }
        IconPos::Bottom => {
            let text_h = (inner_h - icon_size - ICON_GAP).max(1.0);
            let icon = IconRect {
                x: origin_x + (inner_w - icon_size) / 2.0,
                y: origin_y + text_h + ICON_GAP,
                size: icon_size,
            };
            (origin_x, origin_y, inner_w, text_h, Some(icon))
        }
    }
}

pub fn text_align_to_pango(align: &TextAlign) -> pango::Alignment {
    match align {
        TextAlign::Left => pango::Alignment::Left,
        TextAlign::Center => pango::Alignment::Center,
        TextAlign::Right => pango::Alignment::Right,
    }
}
