pub mod color;
pub mod error;
pub mod feed;
pub mod model;
pub mod wayland;

pub use color::{ParseHexRgbaError, parse_hex_rgba, parse_hex_rgba_to_bgra, rgba_to_bgra};
pub use error::PoshankaError;
pub use feed::{FeedEvent, FeedMessage, ParseFeedError, parse_line};
pub use model::{
    CardStyle, IconPos, NotificationView, OverlaySpec, ProgressMode, SubscriberSpec, TextAlign,
    Urgency,
};
pub use wayland::run_overlay;
