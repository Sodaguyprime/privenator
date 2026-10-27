//! Energy-based voice activity detection that cuts the live stream into
//! segments at pauses (docs/SPEC.md "Recording and transcription").

use crate::audio::SAMPLE_RATE;

const RATE: usize = SAMPLE_RATE as usize;
const FRAME: usize = RATE / 50; // 20 ms
/// A pause this long ends a segment.
const PAUSE: usize = RATE * 6 / 10;
/// Segments never grow past this, so Whisper never gets more than its 30 s window.
const MAX_SEGMENT: usize = RATE * 25;
/// Audio kept before the first speech frame, so soft word onsets survive.
const PRE_ROLL: usize = RATE * 3 / 10;
/// Speech must be this far above the tracked noise floor, and above an absolute minimum.
const ABOVE_NOISE_DB: f32 = 12.0;
const MIN_SPEECH_DB: f32 = -45.0;

pub struct Segment {
    /// Offset from the start of the recording.
    pub start_ms: u64,
    pub samples: Vec<f32>,
}

pub struct Segmenter {
    buf: Vec<f32>,
    /// Absolute sample index of `buf[0]`.
    buf_start: u64,
    /// Samples of `buf` already classified (whole frames).
    analyzed: usize,
    speech_seen: bool,
    silence_run: usize,
    noise_db: f32,
}

impl Default for Segmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl Segmenter {
    pub fn new() -> Self {
        Self { buf: Vec::new(), buf_start: 0, analyzed: 0, speech_seen: false, silence_run: 0, noise_db: -60.0 }
    }

    /// Adds audio; returns any segments that ended in it.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Segment> {
        self.buf.extend_from_slice(samples);
        let mut done = Vec::new();
        while self.buf.len() - self.analyzed >= FRAME {
            let db = frame_db(&self.buf[self.analyzed..self.analyzed + FRAME]);
            let speech = self.classify(db);
            self.analyzed += FRAME;

            if !self.speech_seen {
                if speech {
                    self.speech_seen = true;
                    self.silence_run = 0;
                    self.drop_front((self.analyzed - FRAME).saturating_sub(PRE_ROLL));
                } else if self.analyzed > PRE_ROLL {
                    self.drop_front(self.analyzed - PRE_ROLL);
                }
                continue;
            }

            self.silence_run = if speech { 0 } else { self.silence_run + FRAME };
            if self.silence_run >= PAUSE || self.analyzed >= MAX_SEGMENT {
                done.push(self.cut());
            }
        }
        done
    }

    /// Unfinalized speech, for live previews. Empty until speech starts.
    pub fn tail(&self) -> &[f32] {
        if self.speech_seen { &self.buf } else { &[] }
    }

    /// Ends the recording: whatever speech is pending becomes the last segment.
    pub fn flush(&mut self) -> Option<Segment> {
        let segment = if self.speech_seen {
            self.analyzed = self.buf.len();
            Some(self.cut())
        } else {
            None
        };
        self.buf_start += self.buf.len() as u64;
        self.buf.clear();
        self.analyzed = 0;
        segment
    }

    fn classify(&mut self, db: f32) -> bool {
        let speech = db > (self.noise_db + ABOVE_NOISE_DB).max(MIN_SPEECH_DB);
        if !speech {
            self.noise_db = 0.95 * self.noise_db + 0.05 * db;
        }
        speech
    }

    fn cut(&mut self) -> Segment {
        let rest = self.buf.split_off(self.analyzed);
        let samples = std::mem::replace(&mut self.buf, rest);
        let segment = Segment { start_ms: self.buf_start * 1000 / RATE as u64, samples };
        self.buf_start += self.analyzed as u64;
        self.analyzed = 0;
        self.speech_seen = false;
        self.silence_run = 0;
        segment
    }

    fn drop_front(&mut self, n: usize) {
        self.buf.drain(..n);
        self.buf_start += n as u64;
        self.analyzed -= n;
    }
}

fn frame_db(frame: &[f32]) -> f32 {
    let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
    20.0 * rms.max(1e-7).log10()
}
