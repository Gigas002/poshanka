use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::sync::mpsc::SyncSender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tracing::warn;

use crate::model::NotificationView;

use super::command::run_command;
use super::list::parse_list_output;
use super::provider::ProviderSpec;
use super::{FeedMessage, FeedSignal, ParseFeedError, parse_line};

const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Fetch the current notification snapshot via `[provider].command list`.
pub fn fetch_list(spec: &ProviderSpec) -> Result<Vec<NotificationView>, ParseFeedError> {
    let stdout = run_command(spec, &["list"]).map_err(map_command_err)?;
    parse_list_output(&stdout)
}

/// Spawn a background thread that runs `sh -c <exec>` and forwards parsed feed lines.
///
/// Writes one byte to `wakeup` after each successfully parsed message so the Wayland
/// thread can `poll` the wakeup fd and drain the channel without blocking.
pub fn spawn_feed_exec(
    command: String,
    tx: SyncSender<FeedSignal>,
    wakeup: UnixStream,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut backoff = BACKOFF_INITIAL;
        loop {
            match run_feed_once(&command, &tx, &wakeup) {
                Ok(()) => {
                    warn!(%command, "provider feed script exited; restarting");
                }
                Err(e) => {
                    warn!(%command, error = %e, "provider feed error; restarting after backoff");
                }
            }
            thread::sleep(backoff);
            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
    })
}

pub(crate) fn run_feed_once(
    command: &str,
    tx: &SyncSender<FeedSignal>,
    wakeup: &UnixStream,
) -> Result<(), String> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;

    let stdout = child.stdout.take().ok_or("child has no stdout")?;
    let reader = std::io::BufReader::new(stdout);

    for line in reader.lines() {
        let line = line.map_err(|e| format!("read: {e}"))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_line(line) {
            Ok(Some(msg)) => forward_message(msg, tx, wakeup)?,
            Ok(None) => {}
            Err(e) => {
                warn!(%line, error = %e, "provider feed line is not valid NDJSON");
            }
        }
    }

    child.wait().map_err(|e| format!("wait: {e}"))?;
    Ok(())
}

fn forward_message(
    msg: FeedMessage,
    tx: &SyncSender<FeedSignal>,
    wakeup: &UnixStream,
) -> Result<(), String> {
    let signal = match msg {
        FeedMessage::Items(items) => FeedSignal::Items(items),
        FeedMessage::Event(super::FeedEvent::Update { items }) => FeedSignal::Items(items),
        FeedMessage::Event(super::FeedEvent::Reload) => FeedSignal::Reload,
        FeedMessage::Event(super::FeedEvent::HistoryChanged) => return Ok(()),
    };
    tx.try_send(signal)
        .map_err(|e| format!("channel full: {e}"))?;
    wakeup
        .try_clone()
        .map_err(|e| format!("wakeup clone: {e}"))?
        .write_all(&[0])
        .map_err(|e| format!("wakeup write: {e}"))?;
    Ok(())
}

fn map_command_err(err: super::command::CommandError) -> ParseFeedError {
    match err {
        super::command::CommandError::NoCommand => {
            ParseFeedError::Provider("provider command not configured".into())
        }
        super::command::CommandError::Io(e) => ParseFeedError::Provider(format!("io: {e}")),
        super::command::CommandError::Spawn(e) => ParseFeedError::Provider(format!("spawn: {e}")),
        super::command::CommandError::Status { status, stderr } => {
            ParseFeedError::Provider(format!("exit {status}: {stderr}"))
        }
    }
}
