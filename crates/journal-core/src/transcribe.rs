//! Offline speech-to-text with whisper.cpp (docs/SPEC.md "Recording and
//! transcription"). Input is 16 kHz mono f32. Transcripts are journal text:
//! never log them.

use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

#[derive(Debug, thiserror::Error)]
#[error("transcription failed: {0}")]
pub struct TranscribeError(String);

impl From<whisper_rs::WhisperError> for TranscribeError {
    fn from(e: whisper_rs::WhisperError) -> Self {
        Self(e.to_string())
    }
}

pub struct Transcriber {
    state: WhisperState,
    threads: i32,
}

impl Transcriber {
    pub fn load(model: &Path) -> Result<Self, TranscribeError> {
        // Route whisper.cpp's own stderr logging into the `log` crate, which
        // has no logger installed: nothing is printed.
        whisper_rs::install_logging_hooks();
        let path = model.to_str().ok_or_else(|| TranscribeError("model path is not UTF-8".into()))?;
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())?;
        let state = ctx.create_state()?;
        // Leave a core for the UI and audio threads.
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get().saturating_sub(1).clamp(1, 8)) as i32;
        Ok(Self { state, threads })
    }

    /// Transcribes one stretch of speech (up to 30 s) and returns its text.
    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String, TranscribeError> {
        self.run(samples, None)
    }

    /// A faster, rougher pass for live previews: the encoder only looks at as
    /// much audio context as the samples need (docs/SPEC.md, "Preview").
    pub fn preview(&mut self, samples: &[f32]) -> Result<String, TranscribeError> {
        // Whisper's full context is 1500 frames for 30 s.
        let ctx = (samples.len() * 1500 / (16_000 * 30) + 64).min(1500) as i32;
        self.run(samples, Some(ctx))
    }

    fn run(&mut self, samples: &[f32], audio_ctx: Option<i32>) -> Result<String, TranscribeError> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        if let Some(ctx) = audio_ctx {
            params.set_audio_ctx(ctx);
            params.set_single_segment(true);
        }
        params.set_language(Some("en"));
        params.set_n_threads(self.threads);
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        self.state.full(params, samples)?;

        let text: Vec<String> = self
            .state
            .as_iter()
            .map(|s| s.to_string().trim().to_owned())
            .filter(|t| !is_annotation(t))
            .collect();
        Ok(text.join(" ").trim().to_owned())
    }
}

/// Whisper's non-speech markers, such as "[BLANK_AUDIO]" or "(wind blowing)".
fn is_annotation(text: &str) -> bool {
    text.is_empty() || (text.starts_with('[') && text.ends_with(']')) || (text.starts_with('(') && text.ends_with(')'))
}
