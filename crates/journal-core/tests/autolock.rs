use std::time::{Duration, Instant};

use journal_core::autolock::IdleTimer;

const FIVE_MIN: Duration = Duration::from_secs(5 * 60);
const SEC: Duration = Duration::from_secs(1);

#[test]
fn expires_after_timeout_without_activity() {
    let start = Instant::now();
    let timer = IdleTimer::new(FIVE_MIN, start);

    assert!(!timer.expired(start + FIVE_MIN - SEC));
    assert!(timer.expired(start + FIVE_MIN));
}

#[test]
fn activity_restarts_the_countdown() {
    let start = Instant::now();
    let mut timer = IdleTimer::new(FIVE_MIN, start);

    timer.activity(start + 4 * 60 * SEC);

    assert!(!timer.expired(start + FIVE_MIN));
    assert!(timer.expired(start + 4 * 60 * SEC + FIVE_MIN));
}

#[test]
fn hold_prevents_expiry_and_release_restarts_the_countdown() {
    let start = Instant::now();
    let mut timer = IdleTimer::new(FIVE_MIN, start);

    timer.hold();
    assert!(!timer.expired(start + 60 * FIVE_MIN));

    let released = start + 60 * FIVE_MIN;
    timer.release(released);
    assert!(!timer.expired(released + FIVE_MIN - SEC));
    assert!(timer.expired(released + FIVE_MIN));
}
