//! Settings (docs/SPEC.md "Settings"). Password and recovery-key changes are
//! in vault.rs; backup in backup.rs.

use std::time::Instant;

use journal_core::autolock::IdleTimer;
use journal_core::journal::{Settings, SettingsUpdate};
use tauri::State;

use super::{Result, with_journal};
use crate::error::CommandError;
use crate::state::AppState;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    with_journal(&state, |u| Ok(u.journal.settings()?))
}

/// Applies a partial update. A new idle timeout takes effect at once.
#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, update: SettingsUpdate) -> Result<Settings> {
    let mut guard = state.unlocked();
    let u = guard.as_mut().ok_or_else(CommandError::locked)?;
    u.journal.update_settings(&update)?;
    if update.idle_timeout_min.is_some() {
        let held = u.idle.held();
        u.idle = IdleTimer::new(u.journal.idle_timeout()?, Instant::now());
        if held {
            u.idle.hold();
        }
    }
    drop(guard);
    with_journal(&state, |u| Ok(u.journal.settings()?))
}
