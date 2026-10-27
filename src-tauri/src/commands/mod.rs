//! The IPC surface between the webview and Rust. Every command here must also
//! be listed in build.rs and allowed in capabilities/main.json, wrapped in
//! src/lib/ipc.ts, and documented in docs/ARCHITECTURE.md ("Command API").

pub mod backup;
pub mod entries;
pub mod recording;
pub mod settings;
pub mod vault;

use tauri::{AppHandle, Manager};

use crate::error::CommandError;
use crate::state::{AppState, Unlocked};

pub type Result<T> = std::result::Result<T, CommandError>;

/// Runs slow work (Argon2id, Whisper, file I/O) off the async runtime's threads.
pub async fn blocking<T: Send + 'static>(app: AppHandle, f: impl FnOnce(&AppState) -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|e| CommandError::new("internal", e.to_string()))?
}

/// Runs `f` with the unlocked journal, or fails with `locked`.
pub fn with_journal<T>(state: &AppState, f: impl FnOnce(&Unlocked) -> Result<T>) -> Result<T> {
    let guard = state.unlocked();
    f(guard.as_ref().ok_or_else(CommandError::locked)?)
}
