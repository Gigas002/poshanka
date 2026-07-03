/// Phase 0 overlay: solid-color Wayland surface (removed in Phase 4 when real cards arrive).
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

// ── Runtime spec types ────────────────────────────────────────────────────────

/// Subscriber-wide configuration: placement, layer shell, and provider feed wiring.
#[derive(Debug, Clone)]
pub struct SubscriberSpec {
    // [stack]
    pub stack_gap: u32,
    // [placement]
    pub anchor: String,
    pub margin: u32,
    // [layer]
    pub layer: String,
    pub output: String,
    // [provider]
    pub exec: Option<String>,
    pub command: Option<String>,
    pub socket: Option<String>,
}

/// Resolved visual style for a notification card.
///
/// Colors are stored as validated BGRA bytes. Per-notification theme overrides
/// are applied at notification time via `apply_layers`.
#[derive(Debug, Clone)]
pub struct CardStyle {
    // colors
    pub background_bgra: [u8; 4],
    pub foreground_bgra: [u8; 4],
    pub border_bgra: [u8; 4],
    pub progress_bgra: [u8; 4],
    // font
    pub font_name: String,
    pub font_size: f64,
    // layout
    pub width: u32,
    pub height: u32,
    pub padding: u32,
    pub margin: u32,
    // border
    pub border_size: u32,
    pub border_radius: u32,
    // text
    pub text_alignment: TextAlign,
    pub summary_template: String,
    pub body_template: String,
    pub app_template: Option<String>,
    pub id_template: Option<String>,
    // icons
    pub icon_size: i32,
    pub icon_position: IconPos,
    pub icon_theme: String,
    // progress
    pub progress_mode: ProgressMode,
}

/// One notification from a provider feed `list` / subscribe `update` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationView {
    pub id: u32,
    pub app_id: String,
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    pub timeout_ms: Option<u64>,
    pub has_actions: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconPos {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressMode {
    Over,
    Source,
}
