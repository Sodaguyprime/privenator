//! Idle auto-lock policy (docs/SPEC.md "Auto-lock"). Pure logic: the shell
//! passes in the current time and owns the timer thread and the lock itself.

use std::time::{Duration, Instant};

pub struct IdleTimer {
    timeout: Duration,
    last_activity: Instant,
    held: bool,
}

impl IdleTimer {
    pub fn new(timeout: Duration, now: Instant) -> Self {
        Self { timeout, last_activity: now, held: false }
    }

    pub fn activity(&mut self, now: Instant) {
        self.last_activity = now;
    }

    /// Never expire until `release`: while the recovery key is on screen,
    /// and (step 4) while recording.
    pub fn hold(&mut self) {
        self.held = true;
    }

    /// Ends a hold. The full timeout runs again from `now`.
    pub fn release(&mut self, now: Instant) {
        self.held = false;
        self.last_activity = now;
    }

    pub fn held(&self) -> bool {
        self.held
    }

    pub fn expired(&self, now: Instant) -> bool {
        !self.held && now.saturating_duration_since(self.last_activity) >= self.timeout
    }
}
