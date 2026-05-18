/// Plain runtime values for the Phase 0 overlay slice (no config types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlaySpec {
    pub width: u32,
    pub height: u32,
    /// Pixel format for SHM buffer: `[b, g, r, a]` per pixel.
    pub background_bgra: [u8; 4],
}

impl OverlaySpec {
    pub fn new(width: u32, height: u32, background_bgra: [u8; 4]) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            background_bgra,
        }
    }
}
