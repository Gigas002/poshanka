use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor;

/// Screen corner the notification stack is anchored to (`[placement].anchor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    /// Parse a `[placement].anchor` value; unknown values fall back to
    /// `top-right` (mako's default corner).
    pub(crate) fn parse(anchor: &str) -> Self {
        match anchor.trim().to_ascii_lowercase().as_str() {
            "top-left" => Corner::TopLeft,
            "bottom-left" => Corner::BottomLeft,
            "bottom-right" => Corner::BottomRight,
            _ => Corner::TopRight,
        }
    }

    /// `zwlr_layer_surface_v1` anchor bits for this corner.
    pub(crate) fn anchor_bits(self) -> Anchor {
        match self {
            Corner::TopLeft => Anchor::Top | Anchor::Left,
            Corner::TopRight => Anchor::Top | Anchor::Right,
            Corner::BottomLeft => Anchor::Bottom | Anchor::Left,
            Corner::BottomRight => Anchor::Bottom | Anchor::Right,
        }
    }

    /// `(top, right, bottom, left)` margins for a card whose distance from the
    /// anchored edge (cumulative height of earlier cards, plus gaps) is
    /// `stack_offset`.
    pub(crate) fn margins(self, base_margin: u32, stack_offset: u32) -> (i32, i32, i32, i32) {
        let base = base_margin as i32;
        let offset = stack_offset as i32;
        match self {
            Corner::TopLeft => (offset, 0, 0, base),
            Corner::TopRight => (offset, base, 0, 0),
            Corner::BottomLeft => (0, 0, offset, base),
            Corner::BottomRight => (0, base, offset, 0),
        }
    }
}
