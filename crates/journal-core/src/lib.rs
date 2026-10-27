//! Platform-neutral core of the voice journal.
//!
//! Everything sensitive (crypto, storage, audio, transcription) lives here so
//! the desktop shell and the future Android shell share one implementation.
//! This crate must not depend on Tauri or any UI code. See docs/ARCHITECTURE.md.

pub mod audio;
pub mod autolock;
pub mod backup;
mod crypto;
pub mod drafts;
mod entries;
mod hardening;
mod recovery;
pub mod journal;
pub mod live;
pub mod password;
mod search;
pub mod stream;
pub mod transcribe;
pub mod vault;

pub use hardening::harden_process;

/// Version of the core crate, surfaced in the UI so bug reports can name it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
