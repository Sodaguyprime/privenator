//! Entries and search (docs/SPEC.md "Entries", "Search and backup").

use journal_core::audio;
use journal_core::drafts;
use journal_core::journal::{Entry, EntrySummary, NewEntry, SearchHit, SearchQuery, new_entry_id, now_ms};
use tauri::ipc::Response;
use tauri::{AppHandle, State};

use super::{Result, blocking, with_journal};
use crate::error::CommandError;
use crate::state::AppState;

#[tauri::command]
pub fn list_entries(state: State<'_, AppState>) -> Result<Vec<EntrySummary>> {
    with_journal(&state, |u| Ok(u.journal.list_entries()?))
}

#[tauri::command]
pub fn get_entry(state: State<'_, AppState>, id: String) -> Result<Entry> {
    with_journal(&state, |u| u.journal.get_entry(&id)?.ok_or_else(|| CommandError::new("not_found", "No such entry.")))
}

/// "New typed entry": text only, no audio.
#[tauri::command]
pub fn create_typed_entry(state: State<'_, AppState>, text: String) -> Result<String> {
    with_journal(&state, |u| {
        let id = new_entry_id();
        u.journal.create_entry(&NewEntry { id: id.clone(), created_at: now_ms(), text, duration_s: 0, has_audio: false, mood: None, tags: Vec::new() })?;
        Ok(id)
    })
}

#[tauri::command]
pub fn set_entry_title(state: State<'_, AppState>, id: String, title: Option<String>) -> Result<()> {
    with_journal(&state, |u| Ok(u.journal.set_title(&id, title.as_deref())?))
}

/// Saves the latest edit. Text equal to the original clears the edit.
#[tauri::command]
pub fn set_entry_text(state: State<'_, AppState>, id: String, text: String) -> Result<()> {
    with_journal(&state, |u| Ok(u.journal.set_edited_text(&id, &text)?))
}

#[tauri::command]
pub fn revert_entry(state: State<'_, AppState>, id: String) -> Result<()> {
    with_journal(&state, |u| Ok(u.journal.revert_entry(&id)?))
}

#[tauri::command]
pub fn set_entry_mood(state: State<'_, AppState>, id: String, mood: Option<u8>) -> Result<()> {
    with_journal(&state, |u| Ok(u.journal.set_mood(&id, mood)?))
}

#[tauri::command]
pub fn set_entry_tags(state: State<'_, AppState>, id: String, tags: Vec<String>) -> Result<()> {
    with_journal(&state, |u| Ok(u.journal.set_tags(&id, &tags)?))
}

/// Deletes the entry, its tags, search row and audio file.
#[tauri::command]
pub fn delete_entry(state: State<'_, AppState>, id: String) -> Result<()> {
    with_journal(&state, |u| {
        if !u.journal.delete_entry(&id)? {
            return Err(CommandError::new("not_found", "No such entry."));
        }
        match std::fs::remove_file(drafts::audio_path(state.vault.dir(), &id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    })
}

#[tauri::command]
pub fn all_tags(state: State<'_, AppState>) -> Result<Vec<String>> {
    with_journal(&state, |u| Ok(u.journal.all_tags()?))
}

/// The entry's audio as Ogg Opus bytes, decrypted in memory. The UI plays it
/// from a blob: URL; it is never written to disk as plaintext.
#[tauri::command]
pub async fn entry_audio(app: AppHandle, id: String) -> Result<Response> {
    blocking(app, move |state| {
        with_journal(state, |u| {
            let path = drafts::audio_path(state.vault.dir(), &id);
            if !path.exists() {
                return Err(CommandError::new("not_found", "This entry has no audio."));
            }
            let ogg = audio::read_ogg(&path, &u.session)?;
            Ok(Response::new(ogg.bytes.to_vec()))
        })
    })
    .await
}

#[tauri::command]
pub fn search_entries(state: State<'_, AppState>, query: SearchQuery) -> Result<Vec<SearchHit>> {
    with_journal(&state, |u| Ok(u.journal.search(&query)?))
}
