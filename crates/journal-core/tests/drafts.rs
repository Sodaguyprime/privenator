//! Seam: drafts of in-progress recordings, and turning them into entries,
//! including after a crash (docs/SPEC.md "Recording while locked").

use std::f32::consts::TAU;

use journal_core::audio::{self, AudioWriter};
use journal_core::drafts::{self, DraftSegment, DraftWriter};
use journal_core::journal::{Journal, new_entry_id};
use journal_core::vault::{KdfParams, Vault};
use secrecy::SecretString;
use serde_json::json;

const FAST: KdfParams = KdfParams { m_kib: 1024, t: 1, p: 1 };

fn pw() -> SecretString {
    SecretString::from("correct horse battery staple".to_owned())
}

/// Records a draft while locked and "crashes" (never finishes either file).
fn crashed_recording(vault: &Vault, id: &str) {
    let pk = vault.recording_public_key().unwrap();
    let paths = drafts::paths(vault.dir(), id);
    std::fs::create_dir_all(paths.text.parent().unwrap()).unwrap();
    let mut text = DraftWriter::create(&paths.text, &pk).unwrap();
    text.append(&DraftSegment { t_ms: 0, text: "First thought.".into() }).unwrap();
    text.append(&DraftSegment { t_ms: 4_000, text: "Second thought.".into() }).unwrap();
    let mut audio = AudioWriter::create(&paths.audio, &pk).unwrap();
    let tone: Vec<f32> = (0..48_000).map(|i| 0.3 * (TAU * 220.0 * i as f32 / 16_000.0).sin()).collect();
    audio.push(&tone).unwrap();
    drop((text, audio));
}

#[test]
fn a_crashed_recording_becomes_an_entry_at_the_next_unlock() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    drop(vault.create(&pw(), FAST).unwrap());
    let id = new_entry_id();
    crashed_recording(&vault, &id);

    let session = vault.unlock(&pw()).unwrap();
    let journal = Journal::open(vault.dir(), &session).unwrap();
    journal.set_setting("default_keep_audio", &json!(true)).unwrap();
    let pk = vault.recording_public_key().unwrap();

    let recovered = drafts::recover(&journal, &session, &pk, vault.dir(), None).unwrap();

    assert_eq!(recovered.as_slice(), std::slice::from_ref(&id));
    let entry = journal.get_entry(&id).unwrap().unwrap();
    assert_eq!(entry.original_text, "First thought. Second thought.");
    assert!(entry.summary.has_audio);
    assert_eq!(entry.summary.duration_s, 3);
    let kept = audio::read_ogg(&drafts::audio_path(vault.dir(), &id), &session).unwrap();
    assert!(kept.finished, "kept audio is always a finished stream");
    assert_eq!(std::fs::read_dir(vault.dir().join("drafts")).unwrap().count(), 0);
}

#[test]
fn audio_is_discarded_unless_the_setting_says_keep() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    drop(vault.create(&pw(), FAST).unwrap());
    let id = new_entry_id();
    crashed_recording(&vault, &id);

    let session = vault.unlock(&pw()).unwrap();
    let journal = Journal::open(vault.dir(), &session).unwrap();
    drafts::recover(&journal, &session, &vault.recording_public_key().unwrap(), vault.dir(), None).unwrap();

    assert!(!journal.get_entry(&id).unwrap().unwrap().summary.has_audio);
    assert!(!drafts::audio_path(vault.dir(), &id).exists());
}

#[test]
fn the_recording_still_in_progress_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    drop(vault.create(&pw(), FAST).unwrap());
    let id = new_entry_id();
    crashed_recording(&vault, &id);

    let session = vault.unlock(&pw()).unwrap();
    let journal = Journal::open(vault.dir(), &session).unwrap();
    let recovered = drafts::recover(&journal, &session, &vault.recording_public_key().unwrap(), vault.dir(), Some(&id)).unwrap();

    assert!(recovered.is_empty());
    assert!(drafts::paths(vault.dir(), &id).text.exists());
}
