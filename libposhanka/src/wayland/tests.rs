use super::anchor::Corner;
use super::stack::stack_offsets;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor;

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
