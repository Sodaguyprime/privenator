//! Audio: microphone capture, conversion to 16 kHz mono (what Whisper and the
//! Opus encoder take), and the encrypted Opus audio file.
//! Formats: docs/SPEC.md ("Sealed chunk stream", "Audio chunk plaintext").

pub mod capture;
mod convert;
mod file;
mod recorder;

pub use convert::{ConvertError, To16kMono};
pub use file::{AudioError, AudioWriter, OggAudio, decode, read_ogg};
pub use recorder::{Recorder, RecorderError};

/// Everything after capture runs at this rate, mono, f32 in [-1, 1].
pub const SAMPLE_RATE: u32 = 16_000;
