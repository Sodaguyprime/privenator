//! Auto-lock triggers (docs/SPEC.md "Auto-lock"): window minimized, or idle
//! past the timeout. The policy is journal_core::autolock::IdleTimer; this
//! module owns the clock thread and the window hook, and tells the UI.

use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent};

use crate::state::AppState;

/// Rust → UI: keys are gone; clear everything and show the unlock screen.
pub const LOCKED_EVENT: &str = "locked";

pub fn notify_locked(app: &AppHandle) {
    if let Err(e) = app.emit(LOCKED_EVENT, ()) {
        eprintln!("autolock: could not emit {LOCKED_EVENT}: {e}");
    }
}

/// Checks the idle timer once a second for the life of the app. The timer is
/// held while recording, so idle never stops a recording.
pub fn spawn_idle_watcher(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let now = Instant::now();
        if app.state::<AppState>().lock_if(|u| u.idle.expired(now)) {
            notify_locked(&app);
        }
    });
}

/// Locks on minimize, even mid-recording (the recording carries on behind
/// the lock). The one exception: a new recovery key on screen.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if !matches!(event, WindowEvent::Resized(_)) || !window.is_minimized().unwrap_or(false) {
        return;
    }
    let app = window.app_handle();
    if app.state::<AppState>().lock_if(|u| !u.showing_recovery_key) {
        notify_locked(app);
    }
}
