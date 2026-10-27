//! What the shell holds between commands. Locking = dropping `Unlocked`,
//! which zeroizes the keys (journal_core::vault::Session) and closes the DB.
//! A recording lives outside `Unlocked`, so it keeps going while locked.

use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use journal_core::audio::Recorder;
use journal_core::audio::capture::Mic;
use journal_core::autolock::IdleTimer;
use journal_core::journal::{Journal, JournalError};
use journal_core::live::LiveTranscriber;
use journal_core::transcribe::{TranscribeError, Transcriber};
use journal_core::vault::{Session, Vault};
use zeroize::Zeroizing;

pub struct AppState {
    pub vault: Vault,
    pub unlocked: Mutex<Option<Unlocked>>,
    pub throttle: Mutex<Throttle>,
    pub recording: Mutex<Option<ActiveRecording>>,
    /// A stopped recording waiting for the save dialog.
    pub pending: Mutex<Option<PendingSave>>,
    pub mic_test: Mutex<Option<MicTest>>,
    model_path: PathBuf,
    transcriber: Mutex<Option<Arc<Mutex<Transcriber>>>>,
}

pub struct Unlocked {
    pub session: Session,
    pub journal: Journal,
    pub idle: IdleTimer,
    /// A new recovery key is on screen: neither idle nor minimize may lock.
    pub showing_recovery_key: bool,
}

pub struct ActiveRecording {
    pub id: String,
    pub created_at: i64,
    pub recorder: Recorder,
    pub live: LiveTranscriber,
}

pub struct PendingSave {
    pub id: String,
    pub created_at: i64,
    pub duration_s: u32,
    pub text: Zeroizing<String>,
}

pub struct MicTest {
    pub _mic: Mic,
    pub level: Arc<AtomicU32>,
}

impl AppState {
    pub fn new(vault: Vault, model_path: PathBuf) -> Self {
        Self {
            vault,
            unlocked: Mutex::new(None),
            throttle: Mutex::new(Throttle::default()),
            recording: Mutex::new(None),
            pending: Mutex::new(None),
            mic_test: Mutex::new(None),
            model_path,
            transcriber: Mutex::new(None),
        }
    }

    pub fn unlocked(&self) -> MutexGuard<'_, Option<Unlocked>> {
        self.unlocked.lock().expect("state lock poisoned")
    }

    pub fn recording(&self) -> MutexGuard<'_, Option<ActiveRecording>> {
        self.recording.lock().expect("recording lock poisoned")
    }

    /// Returns whether anything was unlocked.
    pub fn lock(&self) -> bool {
        self.lock_if(|_| true)
    }

    /// Locks if `should` says so for the current session. Returns whether it did.
    /// A stopped-but-unsaved recording loses its in-memory transcript; its
    /// encrypted draft becomes an entry at the next unlock.
    pub fn lock_if(&self, should: impl FnOnce(&Unlocked) -> bool) -> bool {
        let mut guard = self.unlocked();
        if guard.as_ref().is_some_and(should) {
            *guard = None;
            drop(guard);
            self.pending.lock().expect("pending lock poisoned").take();
            true
        } else {
            false
        }
    }

    pub fn with_unlocked(&self, f: impl FnOnce(&mut Unlocked)) {
        if let Some(unlocked) = self.unlocked().as_mut() {
            f(unlocked);
        }
    }

    /// Unlocked with a fresh idle timer. The idle timer is held while a
    /// recording runs or a new recovery key is on screen.
    pub fn set_unlocked(&self, session: Session, journal: Journal, showing_recovery_key: bool) -> Result<(), JournalError> {
        let mut idle = IdleTimer::new(journal.idle_timeout()?, Instant::now());
        if showing_recovery_key || self.recording().is_some() {
            idle.hold();
        }
        *self.unlocked() = Some(Unlocked { session, journal, idle, showing_recovery_key });
        Ok(())
    }

    /// Whisper, loaded on first use (~150 MB) and kept for the app's lifetime.
    pub fn transcriber(&self) -> Result<Arc<Mutex<Transcriber>>, TranscribeError> {
        let mut slot = self.transcriber.lock().expect("transcriber lock poisoned");
        if let Some(t) = slot.as_ref() {
            return Ok(t.clone());
        }
        let t = Arc::new(Mutex::new(Transcriber::load(&self.model_path)?));
        *slot = Some(t.clone());
        Ok(t)
    }
}

/// Failed-attempt delay: 1 s, 2 s, 4 s … capped at 30 s (docs/SPEC.md "Rules").
/// In memory only: it's a UX guard, Argon2id cost is the real defense.
#[derive(Default)]
pub struct Throttle {
    failures: u32,
    not_before: Option<Instant>,
}

impl Throttle {
    pub fn wait_remaining(&self) -> Option<Duration> {
        self.not_before.and_then(|t| t.checked_duration_since(Instant::now()))
    }

    pub fn failed(&mut self) -> Duration {
        self.failures += 1;
        let delay = Duration::from_secs((1u64 << (self.failures - 1).min(5)).min(30));
        self.not_before = Some(Instant::now() + delay);
        delay
    }

    pub fn succeeded(&mut self) {
        *self = Self::default();
    }
}
