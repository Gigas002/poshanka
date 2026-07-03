pub mod color;
pub mod error;
pub mod feed;
pub mod model;
pub mod render;
pub mod subscriber;
pub mod wayland;

pub use color::{ParseHexRgbaError, parse_hex_rgba, parse_hex_rgba_to_bgra, rgba_to_bgra};
pub use error::PoshankaError;
pub use feed::{
    CommandError, FeedEvent, FeedMessage, FeedSignal, NotificationState, ParseFeedError,
    ProviderSpec, activate, close, fetch_list, input, parse_line, run_command, spawn_feed_exec,
};
pub use model::{
    CardStyle, IconPos, NotificationView, OverlaySpec, ProgressMode, SubscriberSpec, TextAlign,
    Urgency,
};
pub use render::{ComputedCard, FontContext, Frame, measure_card, paint_card};
pub use subscriber::{SubscriberRun, run as run_subscriber};
pub use wayland::{FeedHandle, run_overlay};
