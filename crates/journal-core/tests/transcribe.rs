//! Seam: speech in, text out (Whisper base.en). Needs the model file:
//! `npm run fetch-model` puts it in src-tauri/resources/models/.

use std::path::PathBuf;

use journal_core::transcribe::Transcriber;

fn model() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/models/ggml-base.en.bin");
    assert!(path.exists(), "missing {}; run `npm run fetch-model`", path.display());
    path
}

/// 16-bit mono PCM WAV → f32 samples.
fn wav(name: &str) -> Vec<f32> {
    let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap();
    let data = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
    bytes[data..].as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0).collect()
}

#[test]
fn speech_becomes_text() {
    let mut transcriber = Transcriber::load(&model()).unwrap();
    let text = transcriber.transcribe(&wav("speech.wav")).unwrap().to_lowercase();
    // "Today I walked along the river and watched the boats."
    for word in ["walked", "river", "boats"] {
        assert!(text.contains(word), "{word:?} missing from {text:?}");
    }
}
