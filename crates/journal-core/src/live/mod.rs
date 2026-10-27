//! Live transcription: segmenting at pauses, previews, and the encrypted draft
//! (docs/SPEC.md "Recording and transcription").

mod pipeline;
mod segmenter;

pub use pipeline::{LiveError, LiveEvent, LiveTranscriber};
pub use segmenter::{Segment, Segmenter};
