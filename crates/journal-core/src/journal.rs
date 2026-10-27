//! The SQLCipher journal database, keyed with the vault's DEK.
//! Schema and connection setup: docs/SPEC.md ("Database schema").

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::vault::Session;

pub use crate::entries::{Entry, EntrySummary, NewEntry, new_entry_id, now_ms};
pub use crate::search::{MATCH_END, MATCH_START, SearchHit, SearchQuery};

const DB_FILE: &str = "journal.db";

/// The idle timeouts Settings offers, in minutes.
pub const IDLE_TIMEOUT_CHOICES_MIN: [u64; 4] = [1, 5, 15, 30];

/// Released migrations, in order. Never edit one after release; append.
const MIGRATIONS: &[&str] = &[include_str!("migrations/0001_initial.sql")];

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("the journal cannot be opened with this key")]
    WrongKey,
    #[error("journal schema is newer than this app (version {0})")]
    TooNew(u32),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("stored value is not valid JSON: {0}")]
    BadValue(#[from] serde_json::Error),
    #[error("no such entry")]
    NotFound,
    #[error("invalid value: {0}")]
    Invalid(String),
}

pub struct Journal {
    pub(crate) conn: Connection,
    /// The raw-key pragma value, kept to key backup copies of the DB.
    key: Zeroizing<String>,
}

impl Journal {
    /// Opens (creating on first use) `journal.db` in `dir`, and migrates it.
    pub fn open(dir: &Path, session: &Session) -> Result<Self, JournalError> {
        let conn = Connection::open(dir.join(DB_FILE))?;

        // Raw-key form, so SQLCipher skips its own KDF: x'<64 hex chars>'.
        let mut key = Zeroizing::new(String::with_capacity(67));
        key.push_str("x'");
        for b in session.dek().iter() {
            write!(key, "{b:02x}").expect("writing to a String cannot fail");
        }
        key.push('\'');
        conn.pragma_update(None, "key", key.as_str())?;

        // The first real read is where a wrong key shows up.
        if let Err(e) = conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)) {
            return Err(match e.sqlite_error_code() {
                Some(rusqlite::ErrorCode::NotADatabase) => JournalError::WrongKey,
                _ => e.into(),
            });
        }

        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;

        let journal = Self { conn, key };
        journal.migrate()?;
        Ok(journal)
    }

    fn migrate(&self) -> Result<(), JournalError> {
        let current: u32 = self.conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let latest = MIGRATIONS.len() as u32;
        if current > latest {
            return Err(JournalError::TooNew(current));
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", i as u32 + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Writes a consistent copy of the DB to `path`, encrypted with the same
    /// key (SQLite online backup API; never a copy of the live file).
    pub fn backup_to(&self, path: &Path) -> Result<(), JournalError> {
        let mut dest = Connection::open(path)?;
        dest.pragma_update(None, "key", self.key.as_str())?;
        rusqlite::backup::Backup::new(&self.conn, &mut dest)?.run_to_completion(1024, Duration::ZERO, None)?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>, JournalError> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?;
        Ok(raw.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    pub fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), JournalError> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value.to_string()],
        )?;
        Ok(())
    }

    /// The `idle_timeout_min` setting as a duration (docs/SPEC.md "Auto-lock").
    /// Unset or not one of the offered choices means the default, 5 minutes.
    pub fn idle_timeout(&self) -> Result<Duration, JournalError> {
        let minutes = self
            .get_setting("idle_timeout_min")?
            .and_then(|v| v.as_u64())
            .filter(|m| IDLE_TIMEOUT_CHOICES_MIN.contains(m))
            .unwrap_or(5);
        Ok(Duration::from_secs(minutes * 60))
    }

    /// All settings with defaults applied (docs/SPEC.md "Settings keys").
    pub fn settings(&self) -> Result<Settings, JournalError> {
        let get = |key| self.get_setting(key);
        Ok(Settings {
            idle_timeout_min: self.idle_timeout()?.as_secs() / 60,
            default_keep_audio: get("default_keep_audio")?.and_then(|v| v.as_bool()).unwrap_or(false),
            mic_device: get("mic_device")?.and_then(|v| v.as_str().map(str::to_owned)),
            backup_reminder_days: get("backup_reminder_days")?.and_then(|v| v.as_u64()).map(|d| d as u32),
            last_backup_at: get("last_backup_at")?.and_then(|v| v.as_i64()),
        })
    }

    /// Applies the fields that are present, after validating all of them.
    pub fn update_settings(&self, update: &SettingsUpdate) -> Result<(), JournalError> {
        if let Some(m) = update.idle_timeout_min
            && !IDLE_TIMEOUT_CHOICES_MIN.contains(&m)
        {
            return Err(JournalError::Invalid(format!("idle timeout must be one of {IDLE_TIMEOUT_CHOICES_MIN:?} minutes")));
        }
        if let Some(Some(d)) = update.backup_reminder_days
            && !(1..=365).contains(&d)
        {
            return Err(JournalError::Invalid("backup reminder must be 1–365 days".into()));
        }
        let tx = self.conn.unchecked_transaction()?;
        if let Some(m) = update.idle_timeout_min {
            self.set_setting("idle_timeout_min", &m.into())?;
        }
        if let Some(keep) = update.default_keep_audio {
            self.set_setting("default_keep_audio", &keep.into())?;
        }
        if let Some(mic) = &update.mic_device {
            self.set_setting("mic_device", &serde_json::json!(mic))?;
        }
        if let Some(days) = update.backup_reminder_days {
            self.set_setting("backup_reminder_days", &serde_json::json!(days))?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Settings {
    pub idle_timeout_min: u64,
    pub default_keep_audio: bool,
    /// cpal device ID; None = system default.
    pub mic_device: Option<String>,
    /// None = reminder off (the default).
    pub backup_reminder_days: Option<u32>,
    pub last_backup_at: Option<i64>,
}

/// A partial update from Settings. For the nullable keys, `Some(None)` clears.
#[derive(Debug, Default, serde::Deserialize)]
pub struct SettingsUpdate {
    pub idle_timeout_min: Option<u64>,
    pub default_keep_audio: Option<bool>,
    #[serde(default, deserialize_with = "nullable")]
    pub mic_device: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub backup_reminder_days: Option<Option<u32>>,
}

/// Distinguishes a JSON `null` (clear) from a missing field (leave alone).
fn nullable<'de, T: serde::Deserialize<'de>, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Ok(Some(Option::deserialize(d)?))
}
