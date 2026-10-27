//! Encrypted backup and restore through native file dialogs opened in Rust
//! (docs/SPEC.md "Search and backup").

use journal_core::backup;
use journal_core::journal::now_ms;
use journal_core::password;
use journal_core::vault::KdfParams;
use secrecy::SecretString;
use tauri::AppHandle;

use super::{Result, blocking, with_journal};
use crate::error::CommandError;

/// Writes a `.vjbak`. With `use_password`, `secret` must be the current
/// journal password (verified); otherwise it is a separate passphrase, held
/// to the same strength rules. Returns false if the dialog was cancelled.
#[tauri::command]
pub async fn create_backup(app: AppHandle, window: tauri::Window, use_password: bool, secret: String) -> Result<bool> {
    let secret = SecretString::from(secret);
    let check = secret.clone();
    blocking(app.clone(), move |state| {
        if use_password {
            drop(state.vault.unlock(&check)?);
        } else {
            let strength = password::strength(secrecy::ExposeSecret::expose_secret(&check));
            if !strength.acceptable {
                return Err(CommandError::new("weak_password", strength.feedback.unwrap_or_else(|| "Choose a stronger passphrase.".into())));
            }
        }
        Ok(())
    })
    .await?;

    let date = utc_date(now_ms());
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_title("Save encrypted backup")
        .set_file_name(format!("privenator-{date}.vjbak"))
        .add_filter("Privenator backup", &["vjbak"])
        .save_file()
        .await
    else {
        return Ok(false);
    };
    let path = file.path().to_owned();

    blocking(app, move |state| {
        with_journal(state, |u| {
            backup::create(state.vault.dir(), &u.journal, &path, &secret, KdfParams::default())?;
            u.journal.set_setting("last_backup_at", &now_ms().into())?;
            Ok(true)
        })
    })
    .await
}

/// Replaces the journal with a backup's. Locks first; refuses while a
/// recording is running. Returns false if the dialog was cancelled. The UI
/// asks for a typed confirmation before calling this when a journal exists.
#[tauri::command]
pub async fn restore_backup(app: AppHandle, window: tauri::Window, passphrase: String) -> Result<bool> {
    let passphrase = SecretString::from(passphrase);
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_title("Restore from backup")
        .add_filter("Privenator backup", &["vjbak"])
        .pick_file()
        .await
    else {
        return Ok(false);
    };
    let path = file.path().to_owned();
    let notify = app.clone();

    blocking(app, move |state| {
        if state.recording().is_some() {
            return Err(CommandError::new("already_recording", "Stop the recording before restoring."));
        }
        if state.lock() {
            crate::autolock::notify_locked(&notify);
        }
        backup::restore(&path, &passphrase, state.vault.dir())?;
        Ok(true)
    })
    .await
}

/// YYYY-MM-DD (UTC) for the default file name, without a date crate.
fn utc_date(unix_ms: i64) -> String {
    // Days since 1970-01-01 → civil date (Howard Hinnant's algorithm).
    let z = unix_ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}
