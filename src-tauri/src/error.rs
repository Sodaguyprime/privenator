//! Errors as the UI sees them: a stable `kind` to branch on, plus a message.
//! Kinds are part of the command API (docs/ARCHITECTURE.md).

use journal_core::audio::capture::CaptureError;
use journal_core::audio::{AudioError, RecorderError};
use journal_core::backup::BackupError;
use journal_core::drafts::DraftError;
use journal_core::journal::JournalError;
use journal_core::live::LiveError;
use journal_core::transcribe::TranscribeError;
use journal_core::vault::{InvalidRecoveryKey, VaultError};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub kind: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

impl CommandError {
    pub fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), retry_after_ms: None }
    }

    pub fn locked() -> Self {
        Self::new("locked", "The journal is locked.")
    }
}

impl From<VaultError> for CommandError {
    fn from(e: VaultError) -> Self {
        let kind = match e {
            VaultError::WrongPassword => "wrong_password",
            VaultError::WrongRecoveryKey => "wrong_recovery_key",
            VaultError::WeakPassword(_) => "weak_password",
            VaultError::AlreadyExists => "already_exists",
            VaultError::NotFound => "no_vault",
            VaultError::Corrupt(_) | VaultError::Io(_) => "internal",
        };
        Self::new(kind, e.to_string())
    }
}

impl From<JournalError> for CommandError {
    fn from(e: JournalError) -> Self {
        let kind = match e {
            JournalError::NotFound => "not_found",
            JournalError::Invalid(_) => "invalid",
            _ => "internal",
        };
        Self::new(kind, e.to_string())
    }
}

impl From<CaptureError> for CommandError {
    fn from(e: CaptureError) -> Self {
        let kind = match e {
            CaptureError::NoDevice => "no_microphone",
            CaptureError::PermissionDenied => "mic_blocked",
            CaptureError::Device(_) => "mic_error",
        };
        Self::new(kind, e.to_string())
    }
}

impl From<RecorderError> for CommandError {
    fn from(e: RecorderError) -> Self {
        match e {
            RecorderError::Capture(c) => c.into(),
            RecorderError::Audio(a) => Self::new("internal", a.to_string()),
        }
    }
}

impl From<BackupError> for CommandError {
    fn from(e: BackupError) -> Self {
        let kind = match e {
            BackupError::WrongPassphrase => "wrong_passphrase",
            BackupError::NotABackup | BackupError::Tampered | BackupError::TooNew => "bad_backup",
            BackupError::Journal(_) | BackupError::Io(_) => "internal",
        };
        Self::new(kind, e.to_string())
    }
}

macro_rules! internal {
    ($($t:ty),*) => {$(
        impl From<$t> for CommandError {
            fn from(e: $t) -> Self {
                Self::new("internal", e.to_string())
            }
        }
    )*};
}
internal!(AudioError, LiveError, DraftError, TranscribeError);

impl From<InvalidRecoveryKey> for CommandError {
    fn from(e: InvalidRecoveryKey) -> Self {
        Self::new("invalid_recovery_key", e.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        Self::new("internal", e.to_string())
    }
}

impl From<tauri::Error> for CommandError {
    fn from(e: tauri::Error) -> Self {
        Self::new("internal", e.to_string())
    }
}
