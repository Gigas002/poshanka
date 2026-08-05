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
    pub icon_default_name: String,
    pub progress_mode: ProgressMode,
    /// Bar thickness in pixels; `0` disables the bar even when a notification
    /// carries a `progress` value.
    pub progress_height: u32,
}

/// One notification from a provider feed `list` / subscribe `update` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationView {
    pub id: u32,
    pub app_id: String,
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    /// Freedesktop `expire_timeout` semantics: `-1` = server default, `0` = never
    /// expire, `>0` = milliseconds. `None` when the provider omits the field.
    pub timeout_ms: Option<i32>,
    pub has_actions: bool,
    pub icon: Option<IconRef>,
    /// FDN `value` hint (progress percent, `0..=100`); `None` when the
    /// provider omits it or reported it out of range.
    pub progress: Option<i32>,
    /// FDN `category` hint (e.g. `"email.arrived"`).
    pub category: Option<String>,
    /// FDN `desktop-entry` hint (desktop file id, no `.desktop` suffix).
    pub desktop_entry: Option<String>,
    /// Whether `body` may contain Pango markup the sender expects rendered
    /// (provider's `body_markup` capability snapshot) rather than escaped
    /// plain text.
    pub body_markup: bool,
}

/// Icon reference from a provider feed payload (`icon.name` / `icon.path` /
/// raw pixel data).
///
/// `raw` (when present) is used directly; otherwise `path` is used directly;
/// otherwise `name` is looked up as an XDG icon-theme name under
/// `CardStyle::icon_theme`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IconRef {
    pub name: Option<String>,
    pub path: Option<String>,
    pub raw: Option<RawIconData>,
}

/// Raw pixel buffer from the FDN `image-data` hint (notred wire `IconRef::Raw`)
/// — used by senders (chat app avatars, etc.) with no icon-theme name or
/// on-disk file. `data_base64` is decoded lazily at render time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawIconData {
    pub width: i32,
    pub height: i32,
    /// Bytes per row, including any padding (may exceed `width * channels`).
    pub rowstride: i32,
    pub has_alpha: bool,
    pub bits_per_sample: i32,
    pub channels: i32,
    pub data_base64: String,
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
