//! Seam: the live transcription pipeline. Audio in (written while locked),
//! finalized segments out, persisted to the encrypted draft. Needs the model
//! (`npm run fetch-model`).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use journal_core::drafts;
use journal_core::live::{LiveEvent, LiveTranscriber};
use journal_core::transcribe::Transcriber;
use journal_core::vault::{KdfParams, Vault};
use secrecy::SecretString;

const FAST: KdfParams = KdfParams { m_kib: 1024, t: 1, p: 1 };

fn pw() -> SecretString {
    SecretString::from("correct horse battery staple".to_owned())
}

fn model() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/models/ggml-base.en.bin");
    assert!(path.exists(), "missing {}; run `npm run fetch-model`", path.display());
    path
}

fn speech() -> Vec<f32> {
    let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/speech.wav")).unwrap();
    let data = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
    bytes[data..].as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0).collect()
}

#[test]
fn speech_with_pauses_becomes_final_segments_in_the_draft() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    drop(vault.create(&pw(), FAST).unwrap()); // locked from here on
    let draft = dir.path().join("live.draft");
    let transcriber = Arc::new(Mutex::new(Transcriber::load(&model()).unwrap()));

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink_events = events.clone();
    let mut live = LiveTranscriber::start(transcriber, &draft, &vault.recording_public_key().unwrap(), move |e| {
        sink_events.lock().unwrap().push(e)
    })
    .unwrap();

    let silence = vec![0.0f32; 16_000];
    let audio = [speech(), silence.clone(), speech(), silence].concat();
    let mut sink = live.take_sink();
    for block in audio.chunks(160) {
        sink(block);
    }
    drop(sink); // recording stopped
    let segments = live.finish().unwrap();

    assert_eq!(segments.len(), 2, "{segments:?}");
    for s in &segments {
        assert!(s.text.to_lowercase().contains("river"), "{:?}", s.text);
    }
    assert!(segments[1].t_ms > 3_000);
    let finals = events.lock().unwrap().iter().filter(|e| matches!(e, LiveEvent::Final(_))).count();
    assert_eq!(finals, 2);

    let session = vault.unlock(&pw()).unwrap();
    assert_eq!(drafts::read_draft(&draft, &session).unwrap(), segments);
}
