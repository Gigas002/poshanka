/// Cumulative margin-from-edge offset for each card, in stacking order.
///
/// `heights[i]` is the pixel height of the i-th card (in provider order, which
/// is treated as the visual stacking order). The returned offset is the
/// distance from the anchored edge to that card's near edge: the first card
/// sits `base_margin` px from the corner, and each following card is pushed
/// out by the sum of the previous cards' heights plus one `gap` each.
pub(crate) fn stack_offsets(heights: &[u32], gap: u32, base_margin: u32) -> Vec<u32> {
    let mut offsets = Vec::with_capacity(heights.len());
    let mut acc = base_margin;
    for &h in heights {
        offsets.push(acc);
        acc = acc.saturating_add(h).saturating_add(gap);
    }
    offsets
}
