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
