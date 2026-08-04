//! Compose per-card frames into a single stack buffer for one `wl_surface`
//! per output, instead of one `wl_surface` per notification.

use crate::render::Frame;

use super::stack::stack_offsets;

/// One card's rendered frame plus its notification id, in the order they
/// should be painted top-to-bottom within the composed stack buffer.
pub(crate) struct StackedFrame {
    pub id: u32,
    pub frame: Frame,
}

/// A card's extent within the composed stack buffer, in surface-local
/// pixels — used for pointer/touch hit-testing against the single surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CardRow {
    pub id: u32,
    pub x_start: u32,
    pub x_end: u32,
    pub y_start: u32,
    pub y_end: u32,
}

/// Compose `frames` (already in top-to-bottom paint order) into one
/// transparent BGRA buffer, stacked with `gap` pixels between cards.
///
/// Cards narrower than the composed width are left-aligned (`right_align =
/// false`, for left-anchored stacks) or right-aligned (`right_align = true`,
/// for right-anchored stacks) — matching mako's per-corner alignment.
///
/// Returns the composed frame and each card's extent for hit-testing.
pub(crate) fn compose_stack(
    frames: &[StackedFrame],
    gap: u32,
    right_align: bool,
) -> (Frame, Vec<CardRow>) {
    if frames.is_empty() {
        return (
            Frame {
                width: 1,
                height: 1,
                stride: 4,
                data: vec![0u8; 4],
            },
            Vec::new(),
        );
    }

    let width = frames
        .iter()
        .map(|f| f.frame.width)
        .max()
        .unwrap_or(1)
        .max(1);
    let heights: Vec<u32> = frames.iter().map(|f| f.frame.height).collect();
    let offsets = stack_offsets(&heights, gap, 0);
    let height = offsets
        .last()
        .zip(heights.last())
        .map(|(&off, &h)| off + h)
        .unwrap_or(0)
        .max(1);

    let stride = (i64::from(width) * 4) as i32;
    let mut data = vec![0u8; (i64::from(stride) * i64::from(height)) as usize];
    let mut rows = Vec::with_capacity(frames.len());

    for (sf, &y) in frames.iter().zip(&offsets) {
        let card_w = sf.frame.width;
        let card_h = sf.frame.height;
        let x = if right_align {
            width.saturating_sub(card_w)
        } else {
            0
        };

        let row_bytes = card_w as usize * 4;
        let src_stride = sf.frame.stride as usize;
        for row in 0..card_h as usize {
            let src_off = row * src_stride;
            let dst_off = (y as usize + row) * stride as usize + x as usize * 4;
            data[dst_off..dst_off + row_bytes]
                .copy_from_slice(&sf.frame.data[src_off..src_off + row_bytes]);
        }

        rows.push(CardRow {
            id: sf.id,
            x_start: x,
            x_end: x + card_w,
            y_start: y,
            y_end: y + card_h,
        });
    }

    (
        Frame {
            width,
            height,
            stride,
            data,
        },
        rows,
    )
}
