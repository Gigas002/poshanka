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

/// When nobody listens any more, the feed stops, and so does its script.
#[test]
fn a_closed_channel_ends_the_run_and_the_script() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("pid");
    use std::os::unix::fs::PermissionsExt;
    let path = dir.path().join("feed.sh");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nwhile true; do echo '{{\"v\": 1, \"type\": \"event\", \"event\": {{\"kind\": \"update\", \"items\": []}}}}'; sleep 0.05; done\n",
            pid_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    drop(rx);
    let (wakeup, _wakeup_rx) = std::os::unix::net::UnixStream::pair().unwrap();
    let mut delivered = 0;
    let result = super::run_feed_once(path.to_str().unwrap(), &tx, &wakeup, &mut delivered);
    assert!(result.is_err());
    let pid = std::fs::read_to_string(&pid_file).unwrap();
    let alive = std::path::Path::new("/proc").join(pid.trim()).exists();
    assert!(!alive, "feed script {} still running", pid.trim());
}
