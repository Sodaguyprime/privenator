//! One recording session: microphone → 16 kHz mono → encrypted Opus file,
//! with every block also handed to a sink (transcription).
//!
//! The audio callback only queues blocks; encoding, disk writes (fsync) and
//! the sink run on a worker thread, so a slow disk can't cause dropouts.
//! Needs only the recording public key: it keeps going while the app is locked.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use super::capture::{self, CaptureError, Mic};
use super::{AudioError, AudioWriter, SAMPLE_RATE};
use crate::vault::RecordingPublicKey;

#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error(transparent)]
    Capture(#[from] CaptureError),
    #[error(transparent)]
    Audio(#[from] AudioError),
}

pub struct Recorder {
    mic: Option<Mic>,
    worker: Option<JoinHandle<Result<(), AudioError>>>,
    level: Arc<AtomicU32>,
    samples: Arc<AtomicU64>,
    paused: bool,
}

impl Recorder {
    /// Starts recording into `audio_path` (a new sealed stream).
    pub fn start(
        audio_path: &Path,
        recipient: &RecordingPublicKey,
        device_id: Option<&str>,
        mut sink: impl FnMut(&[f32]) + Send + 'static,
    ) -> Result<Self, RecorderError> {
        let mut writer = AudioWriter::create(audio_path, recipient)?;
        let level = Arc::new(AtomicU32::new(0));
        let samples = Arc::new(AtomicU64::new(0));

        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let mic = match Mic::open(device_id, move |block| {
            let _ = tx.send(block.to_vec());
        }) {
            Ok(mic) => mic,
            Err(e) => {
                drop(writer);
                let _ = std::fs::remove_file(audio_path);
                return Err(e.into());
            }
        };

        let (worker_level, worker_samples) = (level.clone(), samples.clone());
        let worker = std::thread::Builder::new()
            .name("recorder".into())
            .spawn(move || {
                // Ends when the mic is dropped (its callback owns the sender).
                for block in rx {
                    writer.push(&block)?;
                    worker_samples.fetch_add(block.len() as u64, Ordering::Relaxed);
                    worker_level.store(capture::level(&block).to_bits(), Ordering::Relaxed);
                    sink(&block);
                }
                writer.finish()
            })
            .map_err(|e| RecorderError::Capture(CaptureError::Device(e.to_string())))?;

        Ok(Self { mic: Some(mic), worker: Some(worker), level, samples, paused: false })
    }

    pub fn pause(&mut self) {
        if let Some(mic) = &self.mic {
            mic.pause();
        }
        self.paused = true;
        self.level.store(0, Ordering::Relaxed);
    }

    pub fn resume(&mut self) {
        if let Some(mic) = &self.mic {
            mic.resume();
        }
        self.paused = false;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Current input loudness, 0..1 (see `capture::level`).
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Recorded audio so far, excluding pauses.
    pub fn elapsed(&self) -> Duration {
        Duration::from_millis(self.samples.load(Ordering::Relaxed) * 1000 / SAMPLE_RATE as u64)
    }

    /// Closes the mic, writes the remaining audio and completes the file.
    pub fn stop(mut self) -> Result<Duration, RecorderError> {
        self.mic.take();
        let worker = self.worker.take().expect("worker runs until stop");
        worker.join().map_err(|_| AudioError::Format("recorder thread panicked".into()))??;
        Ok(self.elapsed())
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        // Without stop(): close the mic and let the worker finish the file.
        self.mic.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
