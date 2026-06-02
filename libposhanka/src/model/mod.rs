/// Phase 0 overlay: solid-color Wayland surface (removed in Phase 3 when real cards arrive).
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

// ── Runtime spec types (Phase 1 step 4) ──────────────────────────────────────

/// Daemon-wide configuration resolved from `config.toml` at startup.
#[derive(Debug, Clone)]
pub struct DaemonSpec {
    // [stack]
    pub stack_max: u32,
    // [placement]
    pub anchor: String,
    pub gap: u32,
    pub margin: u32,
    // [queue]
    pub queue_history: bool,
    pub queue_max: u32,
    pub queue_sort: String,
    pub queue_order: String,
    // [timeouts]
    pub timeout_ignore: bool,
    pub timeout_default_ms: u64,
    pub timeout_low_ms: u64,
    pub timeout_normal_ms: u64,
    pub timeout_critical_ms: u64,
    // [layer]
    pub layer: String,
    pub output: String,
}

/// Resolved visual style for a notification card.
///
/// Colors are stored as validated BGRA bytes.  This is the *base* style (from
/// `theme.toml` + base `config.toml` events).  Per-notification overrides are
/// applied at notification time via `apply_layers` / `resolve_events`.
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
    // events
    pub events: CardEvents,
}

/// Resolved shell hooks for card interaction events.
#[derive(Debug, Clone, Default)]
pub struct CardEvents {
    pub on_button_left: Option<String>,
    pub on_button_middle: Option<String>,
    pub on_button_right: Option<String>,
    pub on_notify: Option<String>,
    pub on_touch: Option<String>,
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
