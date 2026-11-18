use std::time::Duration;

use super::{BACKOFF_INITIAL, BACKOFF_MAX, backoff_before_restart};

#[test]
fn backoff_doubles_while_runs_deliver_nothing() {
    let mut backoff = Duration::ZERO;
    let mut seen = Vec::new();
    for _ in 0..7 {
        backoff = backoff_before_restart(backoff, false);
        seen.push(backoff.as_secs());
    }
    assert_eq!(seen, [1, 2, 4, 8, 16, 30, 30]);
    assert_eq!(backoff, BACKOFF_MAX);
}

#[test]
fn backoff_starts_over_after_a_run_that_delivered() {
    assert_eq!(
        backoff_before_restart(Duration::from_secs(30), true),
        BACKOFF_INITIAL
    );
}

/// A feed script printing `n` empty updates, then exiting.
fn script_printing(dir: &std::path::Path, n: usize) -> String {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("feed.sh");
    let line = r#"{"v": 1, "type": "event", "event": {"kind": "update", "items": []}}"#;
    std::fs::write(
        &path,
        format!("#!/bin/sh\n{}", format!("echo '{line}'\n").repeat(n)),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_str().unwrap().to_owned()
}

/// Updates queue up while the Wayland thread is busy painting; the feed
/// waits for room instead of giving up.
#[test]
fn a_full_channel_slows_the_feed_down_instead_of_ending_it() {
    let dir = tempfile::tempdir().unwrap();
    let command = script_printing(dir.path(), 5);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let (wakeup, _wakeup_rx) = std::os::unix::net::UnixStream::pair().unwrap();
    let feed = std::thread::spawn(move || {
        let mut delivered = 0;
        let result = super::run_feed_once(&command, &tx, &wakeup, &mut delivered);
        (result, delivered)
    });
    std::thread::sleep(Duration::from_millis(300));
    let received = rx.iter().take(5).count();
    let (result, delivered) = feed.join().unwrap();
    assert_eq!(result, Ok(()));
    assert_eq!((received, delivered), (5, 5));
}

#[test]
fn log_excerpts_cut_long_lines_on_a_char_boundary() {
    assert_eq!(super::log_excerpt("short"), "short");
    let line = format!("{}é{}", "a".repeat(199), "b".repeat(5_000_000));
    let excerpt = super::log_excerpt(&line);
    assert_eq!(excerpt, format!("{}… (5000201 bytes)", "a".repeat(199)));
}
