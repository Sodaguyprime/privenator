//! Vault lifecycle: setup, unlock, recovery, password and recovery-key changes,
//! locking. Argon2id work runs on a blocking thread so the UI never freezes.

use std::time::Instant;

use journal_core::drafts;
use journal_core::journal::Journal;
use journal_core::password::{self, Strength};
use journal_core::vault::{KdfParams, RecoveryKey, Session};
use secrecy::SecretString;
use serde::Serialize;
use tauri::{AppHandle, State};

use super::{Result, blocking};
use crate::error::CommandError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct AppInfo {
    pub app_version: &'static str,
    pub core_version: &'static str,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        app_version: env!("CARGO_PKG_VERSION"),
        core_version: journal_core::VERSION,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultStatus {
    /// First run: no vault yet.
    None,
    Locked,
    Unlocked,
}

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> VaultStatus {
    if state.unlocked().is_some() {
        VaultStatus::Unlocked
    } else if state.vault.exists() {
        VaultStatus::Locked
    } else {
        VaultStatus::None
    }
}

/// For the strength meter. The password crosses IPC either way when it's set.
#[tauri::command]
pub fn password_strength(password: String) -> Strength {
    password::strength(&password)
}

/// First run. Returns the recovery key to show once.
#[tauri::command]
pub async fn create_vault(app: AppHandle, password: String) -> Result<String> {
    let password = SecretString::from(password);
    blocking(app, move |state| {
        let (session, key) = state.vault.create(&password, KdfParams::default())?;
        open_into(state, session, true)?; // auto-lock held until the key is confirmed
        Ok(key.to_string())
    })
    .await
}

#[derive(Serialize)]
pub struct Unlocked {
    /// Recordings cut short by a crash, turned into entries just now.
    pub recovered: usize,
}

#[tauri::command]
pub async fn unlock(app: AppHandle, password: String) -> Result<Unlocked> {
    let password = SecretString::from(password);
    blocking(app, move |state| {
        if let Some(wait) = state.throttle.lock().expect("throttle poisoned").wait_remaining() {
            return Err(throttled(wait));
        }
        match state.vault.unlock(&password) {
            Ok(session) => {
                state.throttle.lock().expect("throttle poisoned").succeeded();
                let recovered = open_into(state, session, false)?;
                Ok(Unlocked { recovered })
            }
            Err(e) => {
                let mut err = CommandError::from(e);
                if err.kind == "wrong_password" {
                    let wait = state.throttle.lock().expect("throttle poisoned").failed();
                    err.retry_after_ms = Some(wait.as_millis() as u64);
                }
                Err(err)
            }
        }
    })
    .await
}

/// Forgot password: the recovery key sets a new password. Returns the new
/// recovery key, which replaces the used one.
#[tauri::command]
pub async fn recover(app: AppHandle, recovery_key: String, new_password: String) -> Result<String> {
    let key = RecoveryKey::parse(&recovery_key)?;
    let new_password = SecretString::from(new_password);
    blocking(app, move |state| {
        if let Some(wait) = state.throttle.lock().expect("throttle poisoned").wait_remaining() {
            return Err(throttled(wait));
        }
        match state.vault.recover(&key, &new_password) {
            Ok((session, new_key)) => {
                state.throttle.lock().expect("throttle poisoned").succeeded();
                open_into(state, session, true)?; // held until the new key is confirmed
                Ok(new_key.to_string())
            }
            Err(e) => {
                let mut err = CommandError::from(e);
                if err.kind == "wrong_recovery_key" {
                    let wait = state.throttle.lock().expect("throttle poisoned").failed();
                    err.retry_after_ms = Some(wait.as_millis() as u64);
                }
                Err(err)
            }
        }
    })
    .await
}

/// Requires the current password as well as an unlocked session, so someone
/// at an unlocked, unattended machine can't take the journal over.
#[tauri::command]
pub async fn change_password(app: AppHandle, current_password: String, new_password: String) -> Result<()> {
    let current = SecretString::from(current_password);
    let new_password = SecretString::from(new_password);
    blocking(app, move |state| {
        let guard = state.unlocked();
        let unlocked = guard.as_ref().ok_or_else(CommandError::locked)?;
        drop(state.vault.unlock(&current)?); // verifies; the extra session is dropped at once
        state.vault.change_password(&unlocked.session, &new_password)?;
        Ok(())
    })
    .await
}

/// Settings → "Regenerate recovery key". Asks for the current password, like
/// a password change. Returns the new key; auto-lock is held until it's confirmed.
#[tauri::command]
pub async fn regenerate_recovery_key(app: AppHandle, current_password: String) -> Result<String> {
    let current = SecretString::from(current_password);
    blocking(app, move |state| {
        let mut guard = state.unlocked();
        let unlocked = guard.as_mut().ok_or_else(CommandError::locked)?;
        drop(state.vault.unlock(&current)?);
        let key = state.vault.regenerate_recovery_key(&unlocked.session)?;
        unlocked.showing_recovery_key = true;
        unlocked.idle.hold();
        Ok(key.to_string())
    })
    .await
}

#[tauri::command]
pub fn lock(app: AppHandle, state: State<'_, AppState>) {
    if state.lock() {
        crate::autolock::notify_locked(&app);
    }
}

/// Keyboard or pointer input in the webview; restarts the idle countdown.
/// The UI sends it at most every few seconds.
#[tauri::command]
pub fn activity(state: State<'_, AppState>) {
    state.with_unlocked(|u| u.idle.activity(Instant::now()));
}

/// The new recovery key has been confirmed, so auto-lock may resume (idle
/// stays paused if a recording is running).
#[tauri::command]
pub fn release_autolock(state: State<'_, AppState>) {
    let recording = state.recording().is_some();
    state.with_unlocked(|u| {
        u.showing_recovery_key = false;
        if !recording {
            u.idle.release(Instant::now());
        }
    });
}

/// "Save as text file" on the recovery-key screen. A native save dialog
/// opened from Rust, so the webview gets no file-system access.
#[tauri::command]
pub async fn save_recovery_key(window: tauri::Window, recovery_key: String) -> Result<bool> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_title("Save recovery key")
        .set_file_name("privenator-recovery-key.txt")
        .add_filter("Text", &["txt"])
        .save_file()
        .await
    else {
        return Ok(false); // cancelled
    };
    let contents = format!(
        "Privenator recovery key\r\n\r\n{recovery_key}\r\n\r\n\
         Keep this somewhere safe and offline. Anyone holding it can set a new\r\n\
         password for your journal. If you lose both this key and your password,\r\n\
         your journal cannot be opened by anyone.\r\n"
    );
    std::fs::write(file.path(), contents)?;
    Ok(true)
}

/// Opens the DB, turns crash-left drafts into entries, and stores the
/// unlocked state. Returns how many drafts were recovered.
fn open_into(state: &AppState, session: Session, showing_recovery_key: bool) -> Result<usize> {
    let journal = Journal::open(state.vault.dir(), &session)?;
    let in_progress = state.recording().as_ref().map(|r| r.id.clone());
    // Best effort: a damaged draft must never keep the user out of the journal.
    let recovered = drafts::recover(&journal, &session, &state.vault.recording_public_key()?, state.vault.dir(), in_progress.as_deref())
        .unwrap_or_else(|e| {
            eprintln!("draft recovery failed: {e}");
            Vec::new()
        });
    state.set_unlocked(session, journal, showing_recovery_key)?;
    Ok(recovered.len())
}

fn throttled(wait: std::time::Duration) -> CommandError {
    CommandError {
        kind: "throttled",
        message: "Too many attempts. Please wait.".into(),
        retry_after_ms: Some(wait.as_millis() as u64),
    }
}
