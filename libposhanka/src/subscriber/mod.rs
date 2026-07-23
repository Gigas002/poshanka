use std::os::unix::net::UnixStream;
use std::sync::mpsc;

use tracing::{info, warn};

use crate::error::PoshankaError;
use crate::feed::{FeedSignal, ProviderSpec, fetch_list, spawn_feed_exec};
use crate::model::{NotificationView, SubscriberSpec};
use crate::wayland::{FeedHandle, StyleSource, run_overlay};

/// Runtime inputs for the subscriber loop: provider wiring, stack layout, and
/// the per-notification style resolver.
pub struct SubscriberRun {
    pub provider: ProviderSpec,
    pub stack: SubscriberSpec,
    pub style_source: Box<dyn StyleSource>,
}

/// Run the Wayland card stack with an optional provider feed and initial `list` sync.
pub fn run(run: SubscriberRun) -> Result<(), PoshankaError> {
    let mut initial: Vec<NotificationView> = Vec::new();

    if run.provider.command.is_some() {
        match fetch_list(&run.provider) {
            Ok(items) => {
                info!(count = items.len(), "initial provider list");
                initial = items;
            }
            Err(err) => {
                warn!(%err, "initial provider list failed");
            }
        }
    }

    let feed = if let Some(exec) = run.provider.exec.clone() {
        let (wakeup_tx, wakeup_rx) = UnixStream::pair().map_err(|source| PoshankaError::Io {
            path: std::path::PathBuf::from("wakeup-socketpair"),
            source,
        })?;
        let (tx, rx) = mpsc::sync_channel::<FeedSignal>(64);
        let _feed_thread = spawn_feed_exec(exec, tx, wakeup_tx);
        Some(FeedHandle {
            wakeup: wakeup_rx,
            rx,
        })
    } else {
        None
    };

    run_overlay(run.stack, initial, feed, run.style_source)
}
