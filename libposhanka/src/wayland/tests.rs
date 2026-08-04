use super::anchor::Corner;
use super::compose::{CardRow, StackedFrame, compose_stack};
use super::stack::stack_offsets;
use crate::render::Frame;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor;

fn solid_frame(id: u32, width: u32, height: u32, fill: u8) -> StackedFrame {
    let stride = (width * 4) as i32;
    let data = vec![fill; (stride as u64 * height as u64) as usize];
    StackedFrame {
        id,
        frame: Frame {
            width,
            height,
            stride,
            data,
        },
    }
}

// ── Corner::parse ──────────────────────────────────────────────────────────────

#[test]
fn parses_all_known_corners() {
    assert_eq!(Corner::parse("top-left"), Corner::TopLeft);
    assert_eq!(Corner::parse("top-right"), Corner::TopRight);
    assert_eq!(Corner::parse("bottom-left"), Corner::BottomLeft);
    assert_eq!(Corner::parse("bottom-right"), Corner::BottomRight);
}

#[test]
fn parse_is_case_and_whitespace_insensitive() {
    assert_eq!(Corner::parse("  Bottom-Left "), Corner::BottomLeft);
    assert_eq!(Corner::parse("BOTTOM-RIGHT"), Corner::BottomRight);
}

#[test]
fn unknown_anchor_falls_back_to_top_right() {
    assert_eq!(Corner::parse("center"), Corner::TopRight);
    assert_eq!(Corner::parse(""), Corner::TopRight);
}

// ── Corner::anchor_bits ────────────────────────────────────────────────────────

#[test]
fn anchor_bits_match_corner() {
    assert_eq!(Corner::TopLeft.anchor_bits(), Anchor::Top | Anchor::Left);
    assert_eq!(Corner::TopRight.anchor_bits(), Anchor::Top | Anchor::Right);
    assert_eq!(
        Corner::BottomLeft.anchor_bits(),
        Anchor::Bottom | Anchor::Left
    );
    assert_eq!(
        Corner::BottomRight.anchor_bits(),
        Anchor::Bottom | Anchor::Right
    );
}

// ── Corner::margins ─────────────────────────────────────────────────────────────

#[test]
fn top_left_margin_grows_top_keeps_left_fixed() {
    let (top, right, bottom, left) = Corner::TopLeft.margins(16, 40);
    assert_eq!((top, right, bottom, left), (40, 0, 0, 16));
}

#[test]
fn top_right_margin_grows_top_keeps_right_fixed() {
    let (top, right, bottom, left) = Corner::TopRight.margins(16, 40);
    assert_eq!((top, right, bottom, left), (40, 16, 0, 0));
}

#[test]
fn bottom_left_margin_grows_bottom_keeps_left_fixed() {
    let (top, right, bottom, left) = Corner::BottomLeft.margins(16, 40);
    assert_eq!((top, right, bottom, left), (0, 0, 40, 16));
}

#[test]
fn bottom_right_margin_grows_bottom_keeps_right_fixed() {
    let (top, right, bottom, left) = Corner::BottomRight.margins(16, 40);
    assert_eq!((top, right, bottom, left), (0, 16, 40, 0));
}

// ── stack_offsets ────────────────────────────────────────────────────────────

#[test]
fn empty_heights_yields_no_offsets() {
    assert!(stack_offsets(&[], 10, 0).is_empty());
}

#[test]
fn first_card_sits_at_base_margin() {
    let offsets = stack_offsets(&[50], 10, 16);
    assert_eq!(offsets, vec![16]);
}

#[test]
fn cards_stack_with_gap_between_them() {
    let offsets = stack_offsets(&[50, 30, 80], 10, 16);
    // card 0: 16
    // card 1: 16 + 50 + 10 = 76
    // card 2: 76 + 30 + 10 = 116
    assert_eq!(offsets, vec![16, 76, 116]);
}

#[test]
fn zero_gap_and_margin_packs_cards_back_to_back() {
    let offsets = stack_offsets(&[20, 20, 20], 0, 0);
    assert_eq!(offsets, vec![0, 20, 40]);
}

#[test]
fn saturates_instead_of_overflowing() {
    let offsets = stack_offsets(&[u32::MAX, 10], u32::MAX, u32::MAX);
    assert_eq!(offsets[0], u32::MAX);
    assert_eq!(offsets[1], u32::MAX);
}

// ── Corner::is_top / is_right ───────────────────────────────────────────────────

#[test]
fn is_top_true_only_for_top_corners() {
    assert!(Corner::TopLeft.is_top());
    assert!(Corner::TopRight.is_top());
    assert!(!Corner::BottomLeft.is_top());
    assert!(!Corner::BottomRight.is_top());
}

#[test]
fn is_right_true_only_for_right_corners() {
    assert!(Corner::TopRight.is_right());
    assert!(Corner::BottomRight.is_right());
    assert!(!Corner::TopLeft.is_right());
    assert!(!Corner::BottomLeft.is_right());
}

// ── compose_stack ────────────────────────────────────────────────────────────

#[test]
fn compose_stack_empty_input_yields_1x1_and_no_rows() {
    let (frame, rows) = compose_stack(&[], 10, false);
    assert_eq!((frame.width, frame.height), (1, 1));
    assert!(rows.is_empty());
}

#[test]
fn compose_stack_single_frame_matches_its_own_size() {
    let frames = vec![solid_frame(1, 100, 40, 0xAA)];
    let (frame, rows) = compose_stack(&frames, 10, false);
    assert_eq!((frame.width, frame.height), (100, 40));
    assert_eq!(
        rows,
        vec![CardRow {
            id: 1,
            x_start: 0,
            x_end: 100,
            y_start: 0,
            y_end: 40,
        }]
    );
}

#[test]
fn compose_stack_stacks_vertically_with_gap() {
    let frames = vec![solid_frame(1, 100, 40, 0xAA), solid_frame(2, 100, 30, 0xBB)];
    let (frame, rows) = compose_stack(&frames, 10, false);
    // height = 40 + gap(10) + 30
    assert_eq!(frame.height, 80);
    assert_eq!(rows[0].y_start, 0);
    assert_eq!(rows[0].y_end, 40);
    assert_eq!(rows[1].y_start, 50);
    assert_eq!(rows[1].y_end, 80);
}

#[test]
fn compose_stack_width_is_the_widest_card() {
    let frames = vec![solid_frame(1, 100, 20, 0xAA), solid_frame(2, 60, 20, 0xBB)];
    let (frame, _) = compose_stack(&frames, 0, false);
    assert_eq!(frame.width, 100);
}

#[test]
fn compose_stack_left_aligns_narrower_cards_by_default() {
    let frames = vec![solid_frame(1, 100, 20, 0xAA), solid_frame(2, 60, 20, 0xBB)];
    let (_, rows) = compose_stack(&frames, 0, false);
    assert_eq!(rows[1].x_start, 0);
    assert_eq!(rows[1].x_end, 60);
}

#[test]
fn compose_stack_right_aligns_narrower_cards_when_requested() {
    let frames = vec![solid_frame(1, 100, 20, 0xAA), solid_frame(2, 60, 20, 0xBB)];
    let (_, rows) = compose_stack(&frames, 0, true);
    assert_eq!(rows[1].x_start, 40);
    assert_eq!(rows[1].x_end, 100);
}

#[test]
fn compose_stack_copies_pixel_data_into_correct_position() {
    let frames = vec![solid_frame(1, 10, 5, 0x11), solid_frame(2, 10, 5, 0x22)];
    let (frame, rows) = compose_stack(&frames, 0, false);

    // Sample a pixel from card 1's region (top) and card 2's region (bottom).
    let px_at = |x: u32, y: u32| -> u8 {
        let off = y as usize * frame.stride as usize + x as usize * 4;
        frame.data[off]
    };
    assert_eq!(px_at(0, rows[0].y_start), 0x11);
    assert_eq!(px_at(0, rows[1].y_start), 0x22);
}

#[test]
fn compose_stack_leaves_gap_transparent() {
    let frames = vec![solid_frame(1, 10, 5, 0xFF), solid_frame(2, 10, 5, 0xFF)];
    let (frame, rows) = compose_stack(&frames, 6, false);
    // A row strictly inside the gap between the two cards should stay zeroed
    // (fully transparent), not bleed either card's fill color.
    let gap_y = rows[0].y_end + 2;
    assert!(gap_y < rows[1].y_start);
    let off = gap_y as usize * frame.stride as usize;
    assert_eq!(frame.data[off], 0);
}
