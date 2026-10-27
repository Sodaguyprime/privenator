//! In-progress recordings and how they become entries (docs/SPEC.md
//! "Recording while locked", "Files").
//!
//! A recording writes two sealed streams under `drafts/`: the finalized
//! transcript segments (`<id>.draft`) and the audio (`<id>.opus.enc`). Both
//! need only the recording public key. Saving turns them into an entry with
//! the same ID; kept audio moves to `audio/<id>.opus.enc`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::audio::{self, SAMPLE_RATE};
use crate::journal::{Journal, JournalError, NewEntry};
use crate::stream::{self, StreamError, StreamWriter};
use crate::vault::{RecordingPublicKey, Session};

#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    #[error(transparent)]
    Stream(#[from] StreamError),
    #[error(transparent)]
    Audio(#[from] audio::AudioError),
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error("draft segment is not valid JSON: {0}")]
    Format(#[from] serde_json::Error),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

/// One finalized transcript segment, as stored in a draft chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftSegment {
    /// Start, from the beginning of the recording.
    pub t_ms: u64,
    pub text: String,
}

pub struct DraftPaths {
    pub text: PathBuf,
    pub audio: PathBuf,
}

pub fn paths(data_dir: &Path, id: &str) -> DraftPaths {
    let dir = data_dir.join("drafts");
    DraftPaths { text: dir.join(format!("{id}.draft")), audio: dir.join(format!("{id}.opus.enc")) }
}

/// Where an entry's kept audio lives.
pub fn audio_path(data_dir: &Path, id: &str) -> PathBuf {
    data_dir.join("audio").join(format!("{id}.opus.enc"))
}

pub struct DraftWriter(StreamWriter);

impl DraftWriter {
    pub fn create(path: &Path, recipient: &RecordingPublicKey) -> Result<Self, DraftError> {
        Ok(Self(StreamWriter::create(path, recipient)?))
    }

    /// Appends one segment as its own chunk (synced to disk).
    pub fn append(&mut self, segment: &DraftSegment) -> Result<(), DraftError> {
        let mut json = serde_json::to_vec(segment)?;
        let result = self.0.append(&json);
        json.zeroize();
        Ok(result?)
    }

    pub fn finish(self) -> Result<(), DraftError> {
        Ok(self.0.finish()?)
    }
}

pub fn read_draft(path: &Path, session: &Session) -> Result<Vec<DraftSegment>, DraftError> {
    let contents = stream::read(path, session)?;
    contents.chunks.iter().map(|c| Ok(serde_json::from_slice(c)?)).collect()
}

/// The transcript text of an entry, from its segments.
pub fn join_segments(segments: &[DraftSegment]) -> String {
    segments.iter().map(|s| s.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Saves a recording as an entry (`entry.id` is the draft ID): keeps or
/// deletes its audio, inserts the entry, then removes the draft files.
pub fn commit(
    journal: &Journal,
    session: &Session,
    recipient: &RecordingPublicKey,
    data_dir: &Path,
    mut entry: NewEntry,
    keep_audio: bool,
) -> Result<(), DraftError> {
    let draft = paths(data_dir, &entry.id);
    entry.has_audio = keep_audio && draft.audio.exists() && keep(session, recipient, &draft.audio, &audio_path(data_dir, &entry.id))?;
    journal.create_entry(&entry)?;
    remove_if_exists(&draft.audio)?;
    remove_if_exists(&draft.text)?;
    Ok(())
}

/// Turns drafts left by a crash into entries. `in_progress` is the recording
/// that is still running, if any; it is left alone. Returns the new entry IDs.
pub fn recover(
    journal: &Journal,
    session: &Session,
    recipient: &RecordingPublicKey,
    data_dir: &Path,
    in_progress: Option<&str>,
) -> Result<Vec<String>, DraftError> {
    let Ok(dir) = fs::read_dir(data_dir.join("drafts")) else { return Ok(Vec::new()) };
    let mut ids: Vec<String> = dir
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(".draft").or_else(|| name.strip_suffix(".opus.enc")).map(str::to_owned))
        .filter(|id| Some(id.as_str()) != in_progress)
        .collect();
    ids.sort();
    ids.dedup();

    let keep_audio = journal.get_setting("default_keep_audio")?.and_then(|v| v.as_bool()).unwrap_or(false);
    let mut recovered = Vec::new();
    for id in ids {
        let draft = paths(data_dir, &id);
        let segments = if draft.text.exists() { read_draft(&draft.text, session)? } else { Vec::new() };
        let duration_s = if draft.audio.exists() {
            let ogg = audio::read_ogg(&draft.audio, session)?;
            ((audio::decode(&ogg.bytes)?.len() as u64 + SAMPLE_RATE as u64 / 2) / SAMPLE_RATE as u64) as u32
        } else {
            segments.last().map_or(0, |s| (s.t_ms / 1000) as u32)
        };
        let created_at = uuid::Uuid::parse_str(&id)
            .ok()
            .and_then(|u| u.get_timestamp())
            .map_or_else(crate::entries::now_ms, |t| {
                let (secs, nanos) = t.to_unix();
                (secs * 1000 + nanos as u64 / 1_000_000) as i64
            });
        let entry = NewEntry {
            id: id.clone(),
            created_at,
            text: join_segments(&segments),
            duration_s,
            has_audio: false,
            mood: None,
            tags: Vec::new(),
        };
        commit(journal, session, recipient, data_dir, entry, keep_audio)?;
        recovered.push(id);
    }
    Ok(recovered)
}

/// Moves draft audio to its kept location. A stream cut short by a crash is
/// rewritten as a finished one, since kept audio must always be finished.
/// Returns false if nothing playable was recorded.
fn keep(session: &Session, recipient: &RecordingPublicKey, from: &Path, to: &Path) -> Result<bool, DraftError> {
    fs::create_dir_all(to.parent().expect("audio/ has a parent"))?;
    let contents = stream::read(from, session)?;
    if contents.chunks.len() <= 1 {
        return Ok(false); // headers only, no audio
    }
    if contents.finished {
        fs::rename(from, to)?;
        return Ok(true);
    }
    let tmp = to.with_extension("enc.tmp");
    let mut writer = StreamWriter::create(&tmp, recipient)?;
    for chunk in &contents.chunks {
        writer.append(chunk)?;
    }
    writer.finish()?;
    fs::rename(&tmp, to)?;
    Ok(true)
}

fn remove_if_exists(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}
