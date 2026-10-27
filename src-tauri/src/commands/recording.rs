//! Recording: microphone choice and test, start/pause/resume/stop, and saving
//! the result as an entry (docs/SPEC.md "Recording and transcription").
//!
//! A recording writes only to sealed streams (public key), so it keeps going
//! while locked. Transcript text is only sent to the UI while unlocked.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use journal_core::audio::Recorder;
use journal_core::audio::capture::{self, InputDevice, Mic};
use journal_core::drafts::{self, DraftSegment};

use journal_core::journal::{NewEntry, new_entry_id, now_ms};
use journal_core::live::{LiveEvent, LiveTranscriber};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use zeroize::Zeroizing;

use super::{Result, blocking, with_journal};
use crate::error::CommandError;
use crate::state::{ActiveRecording, AppState, MicTest, PendingSave};

pub const PREVIEW_EVENT: &str = "transcript:preview";
pub const FINAL_EVENT: &str = "transcript:final";

#[tauri::command]
pub fn list_microphones() -> Vec<InputDevice> {
    capture::input_devices()
}

/// Opens the mic for the level meter only (first run, Settings). Nothing is stored.
#[tauri::command]
pub fn mic_test_start(state: State<'_, AppState>, device_id: Option<String>) -> Result<()> {
    if state.recording().is_some() {
        return Err(CommandError::new("already_recording", "A recording is in progress."));
    }
    let mut slot = state.mic_test.lock().expect("mic test lock poisoned");
    slot.take(); // close any previous test first
    let level = Arc::new(AtomicU32::new(0));
    let meter = level.clone();
    let mic = Mic::open(device_id.as_deref(), move |block| meter.store(capture::level(block).to_bits(), Ordering::Relaxed))?;
    *slot = Some(MicTest { _mic: mic, level });
    Ok(())
}

#[tauri::command]
pub fn mic_test_stop(state: State<'_, AppState>) {
    state.mic_test.lock().expect("mic test lock poisoned").take();
}

/// Starts recording. Returns the recording's ID (also the future entry ID).
#[tauri::command]
pub async fn start_recording(app: AppHandle) -> Result<String> {
    let events = app.clone();
    blocking(app, move |state| {
        let mic_device = with_journal(state, |u| Ok(u.journal.settings()?.mic_device))?;
        if state.recording().is_some() || state.pending.lock().expect("pending poisoned").is_some() {
            return Err(CommandError::new("already_recording", "A recording is already in progress."));
        }
        state.mic_test.lock().expect("mic test lock poisoned").take();

        let transcriber = state.transcriber()?;
        let pk = state.vault.recording_public_key()?;
        let id = new_entry_id();
        let paths = drafts::paths(state.vault.dir(), &id);
        std::fs::create_dir_all(paths.text.parent().expect("drafts/ has a parent"))?;

        let mut live = LiveTranscriber::start(transcriber, &paths.text, &pk, move |event| emit_if_unlocked(&events, event))?;
        let recorder = match Recorder::start(&paths.audio, &pk, mic_device.as_deref(), live.take_sink()) {
            Ok(r) => r,
            Err(e) => {
                let _ = live.finish();
                let _ = std::fs::remove_file(&paths.text);
                return Err(e.into());
            }
        };
        *state.recording() = Some(ActiveRecording { id: id.clone(), created_at: now_ms(), recorder, live });
        state.with_unlocked(|u| u.idle.hold()); // idle never interrupts a recording
        Ok(id)
    })
    .await
}

#[tauri::command]
pub fn pause_recording(state: State<'_, AppState>) -> Result<()> {
    let mut rec = state.recording();
    rec.as_mut().ok_or_else(not_recording)?.recorder.pause();
    Ok(())
}

#[tauri::command]
pub fn resume_recording(state: State<'_, AppState>) -> Result<()> {
    let mut rec = state.recording();
    rec.as_mut().ok_or_else(not_recording)?.recorder.resume();
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingState {
    Idle,
    Recording,
    Paused,
    /// Stopped; the save dialog is next.
    Stopped,
}

#[derive(Serialize)]
pub struct RecordingStatus {
    pub state: RecordingState,
    pub id: Option<String>,
    pub elapsed_ms: u64,
    /// Final text so far, to repopulate the screen after an unlock.
    pub finals: Vec<DraftSegment>,
    pub stopped: Option<Stopped>,
}

/// What the UI needs to rebuild the recording screen (e.g. after unlocking).
#[tauri::command]
pub fn recording_status(state: State<'_, AppState>) -> Result<RecordingStatus> {
    if state.unlocked().is_none() {
        return Err(CommandError::locked());
    }
    if let Some(r) = state.recording().as_ref() {
        return Ok(RecordingStatus {
            state: if r.recorder.is_paused() { RecordingState::Paused } else { RecordingState::Recording },
            id: Some(r.id.clone()),
            elapsed_ms: r.recorder.elapsed().as_millis() as u64,
            finals: r.live.finals(),
            stopped: None,
        });
    }
    if let Some(p) = state.pending.lock().expect("pending poisoned").as_ref() {
        return Ok(RecordingStatus {
            state: RecordingState::Stopped,
            id: Some(p.id.clone()),
            elapsed_ms: p.duration_s as u64 * 1000,
            finals: Vec::new(),
            stopped: Some(Stopped { id: p.id.clone(), duration_s: p.duration_s, text: p.text.to_string() }),
        });
    }
    Ok(RecordingStatus { state: RecordingState::Idle, id: None, elapsed_ms: 0, finals: Vec::new(), stopped: None })
}

#[derive(Serialize)]
pub struct Stopped {
    pub id: String,
    pub duration_s: u32,
    pub text: String,
}

/// Stops the mic, transcribes what's left and completes the draft. The entry
/// is created by `save_recording`, after the save dialog.
#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> Result<Stopped> {
    blocking(app, move |state| {
        if state.unlocked().is_none() {
            return Err(CommandError::locked());
        }
        let rec = state.recording().take().ok_or_else(not_recording)?;
        let duration = rec.recorder.stop()?;
        let segments = rec.live.finish()?;
        let text = Zeroizing::new(drafts::join_segments(&segments));
        let duration_s = duration.as_secs_f32().round() as u32;
        let stopped = Stopped { id: rec.id.clone(), duration_s, text: text.to_string() };
        *state.pending.lock().expect("pending poisoned") = Some(PendingSave { id: rec.id, created_at: rec.created_at, duration_s, text });
        state.with_unlocked(|u| {
            if !u.showing_recovery_key {
                u.idle.release(Instant::now());
            }
        });
        Ok(stopped)
    })
    .await
}

/// The save dialog: keep audio?, mood, tags. Returns the new entry's ID.
#[tauri::command]
pub async fn save_recording(app: AppHandle, keep_audio: bool, mood: Option<u8>, tags: Vec<String>) -> Result<String> {
    blocking(app, move |state| {
        let pending = state.pending.lock().expect("pending poisoned").take().ok_or_else(not_recording)?;
        let pk = state.vault.recording_public_key()?;
        with_journal(state, |u| {
            let entry = NewEntry {
                id: pending.id.clone(),
                created_at: pending.created_at,
                text: pending.text.to_string(),
                duration_s: pending.duration_s,
                has_audio: false,
                mood,
                tags,
            };
            drafts::commit(&u.journal, &u.session, &pk, state.vault.dir(), entry, keep_audio)?;
            Ok(pending.id.clone())
        })
    })
    .await
}

fn emit_if_unlocked(app: &AppHandle, event: LiveEvent) {
    if app.state::<AppState>().unlocked().is_none() {
        return; // the transcript stays in Rust while locked
    }
    let result = match event {
        LiveEvent::Preview(text) => app.emit(PREVIEW_EVENT, text),
        LiveEvent::Final(segment) => app.emit(FINAL_EVENT, segment),
    };
    if let Err(e) = result {
        eprintln!("recording: could not emit transcript event: {e}");
    }
}

fn not_recording() -> CommandError {
    CommandError::new("not_recording", "No recording in progress.")
}

/// Meter thread: ~10 updates a second while recording or testing the mic.
pub fn spawn_level_meter(app: AppHandle) {
    #[derive(Clone, Serialize)]
    struct Level {
        level: f32,
        elapsed_ms: u64,
        paused: bool,
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let state = app.state::<AppState>();
        let level = if let Some(r) = state.recording().as_ref() {
            Some(Level { level: r.recorder.level(), elapsed_ms: r.recorder.elapsed().as_millis() as u64, paused: r.recorder.is_paused() })
        } else {
            state
                .mic_test
                .lock()
                .expect("mic test lock poisoned")
                .as_ref()
                .map(|t| Level { level: f32::from_bits(t.level.load(Ordering::Relaxed)), elapsed_ms: 0, paused: false })
        };
        if let Some(level) = level {
            let _ = app.emit("recording:level", level);
        }
    });
}
