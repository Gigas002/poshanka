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
        let mut backoff = Duration::ZERO;
        loop {
            let mut delivered = 0;
            match run_feed_once(&command, &tx, &wakeup, &mut delivered) {
                Ok(()) => {
                    warn!(%command, "provider feed script exited; restarting");
                }
                Err(e) => {
                    warn!(%command, error = %e, "provider feed error; restarting after backoff");
                }
            }
            backoff = backoff_before_restart(backoff, delivered > 0);
            thread::sleep(backoff);
        }
    })
}

/// The delay before restarting the feed script: it doubles (up to
/// [`BACKOFF_MAX`]) while runs deliver nothing, and starts over at
/// [`BACKOFF_INITIAL`] after a run that delivered messages, so a provider
/// that restarts now and then is reconnected to quickly every time.
pub(crate) fn backoff_before_restart(previous: Duration, delivered: bool) -> Duration {
    if delivered || previous.is_zero() {
        BACKOFF_INITIAL
    } else {
        (previous * 2).min(BACKOFF_MAX)
    }
}

/// Run the feed script once, forwarding its messages, until it exits;
/// `delivered` counts the messages forwarded.
pub(crate) fn run_feed_once(
    command: &str,
    tx: &SyncSender<FeedSignal>,
    wakeup: &UnixStream,
    delivered: &mut usize,
) -> Result<(), String> {
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    end_with_parent(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;

    let stdout = child.stdout.take().ok_or("child has no stdout")?;
    let result = forward_lines(std::io::BufReader::new(stdout), tx, wakeup, delivered);
    if result.is_err() {
        // Don't leave the script running, writing to a pipe nobody reads.
        let _ = child.kill();
    }
    child.wait().map_err(|e| format!("wait: {e}"))?;
    result
}

/// Have the kernel send the feed script `SIGTERM` when poshanka ends,
/// however it ends (crash, `SIGKILL`, compositor gone), so the script and
/// its subscription don't linger without a reader. (The signal follows the
/// thread that spawned the script: the feed thread, which lives as long as
/// poshanka.)
fn end_with_parent(cmd: &mut Command) {
    use rustix::process::{Signal, set_parent_process_death_signal};
    use std::os::unix::process::CommandExt;
    // SAFETY: the closure runs between fork and exec, where only
    // async-signal-safe calls are allowed; it makes one prctl system call
    // and touches no memory shared with the parent.
    unsafe {
        cmd.pre_exec(|| {
            set_parent_process_death_signal(Some(Signal::TERM)).map_err(std::io::Error::from)
        });
    }
}

fn forward_lines(
    reader: impl BufRead,
    tx: &SyncSender<FeedSignal>,
    wakeup: &UnixStream,
    delivered: &mut usize,
) -> Result<(), String> {
    for line in reader.lines() {
        let line = line.map_err(|e| format!("read: {e}"))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_line(line) {
            Ok(Some(msg)) => {
                forward_message(msg, tx, wakeup)?;
                *delivered += 1;
            }
            Ok(None) => {}
            Err(e) => {
                warn!(%line, error = %e, "provider feed line is not valid NDJSON");
            }
        }
    }
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
    // Waits while the Wayland thread is busy with earlier updates; fails
    // only when it is gone.
    tx.send(signal)
        .map_err(|_| "the Wayland loop has ended".to_string())?;
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

#[cfg(test)]
mod tests;
