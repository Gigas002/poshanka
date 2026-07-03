use std::os::unix::net::UnixStream;
use std::sync::mpsc;

use tracing::{info, warn};

use crate::error::PoshankaError;
use crate::feed::{FeedSignal, NotificationState, ProviderSpec, fetch_list, spawn_feed_exec};
use crate::model::OverlaySpec;
use crate::wayland::{FeedHandle, run_overlay};

/// Runtime inputs for the Phase 3 subscriber loop (overlay + provider feed).
#[derive(Debug, Clone)]
pub struct SubscriberRun {
    pub provider: ProviderSpec,
    pub overlay: OverlaySpec,
}

/// Run the Wayland overlay loop with an optional provider feed and initial `list` sync.
pub fn run(run: SubscriberRun) -> Result<(), PoshankaError> {
    let mut notifications = NotificationState::default();

    if run.provider.command.is_some() {
        match fetch_list(&run.provider) {
            Ok(items) => {
                notifications.replace(items);
                info!(count = notifications.len(), "initial provider list");
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
            notifications,
        })
    } else {
        if !notifications.is_empty() {
            info!(count = notifications.len(), "provider list snapshot");
        }
        None
    };

    run_overlay(run.overlay, feed)
}
