//! The live transcription worker: finals before previews, finals persisted
//! to the encrypted draft as they happen.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::{Segment, Segmenter};
use crate::audio::SAMPLE_RATE;
use crate::drafts::{DraftError, DraftSegment, DraftWriter};
use crate::transcribe::{TranscribeError, Transcriber};
use crate::vault::RecordingPublicKey;

const PREVIEW_EVERY: Duration = Duration::from_millis(1500);
/// Previews only look at the end of the unfinished segment.
const PREVIEW_WINDOW: usize = SAMPLE_RATE as usize * 8;
const PREVIEW_MIN: usize = SAMPLE_RATE as usize;

#[derive(Debug, thiserror::Error)]
pub enum LiveError {
    #[error(transparent)]
    Draft(#[from] DraftError),
    #[error(transparent)]
    Transcribe(#[from] TranscribeError),
    #[error("transcription thread failed")]
    Panicked,
}

/// What the UI shows while recording. Journal text: only deliver it to an
/// unlocked UI, and never log it.
#[derive(Debug, Clone)]
pub enum LiveEvent {
    /// Provisional text for the speech since the last final; may change.
    Preview(String),
    /// A finished segment, already saved to the draft.
    Final(DraftSegment),
}

pub struct LiveTranscriber {
    sink: Option<mpsc::Sender<Vec<f32>>>,
    finals: Arc<Mutex<Vec<DraftSegment>>>,
    worker: JoinHandle<Result<Vec<DraftSegment>, LiveError>>,
}

impl LiveTranscriber {
    pub fn start(
        transcriber: Arc<Mutex<Transcriber>>,
        draft_path: &Path,
        recipient: &RecordingPublicKey,
        on_event: impl FnMut(LiveEvent) + Send + 'static,
    ) -> Result<Self, LiveError> {
        let draft = DraftWriter::create(draft_path, recipient)?;
        let finals = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let worker_finals = finals.clone();
        let worker = std::thread::Builder::new()
            .name("transcribe".into())
            .spawn(move || run(rx, transcriber, draft, worker_finals, on_event))
            .map_err(|_| LiveError::Panicked)?;
        Ok(Self { sink: Some(tx), finals, worker })
    }

    /// The audio input, for the recorder. Dropping it ends the recording.
    pub fn take_sink(&mut self) -> impl FnMut(&[f32]) + Send + 'static {
        let tx = self.sink.take().expect("sink taken once");
        move |block: &[f32]| {
            let _ = tx.send(block.to_vec());
        }
    }

    /// Final segments so far (to repopulate the UI after an unlock).
    pub fn finals(&self) -> Vec<DraftSegment> {
        self.finals.lock().expect("finals poisoned").clone()
    }

    /// Waits for the remaining audio to be transcribed (after the sink is
    /// dropped) and completes the draft. Returns every final segment.
    pub fn finish(mut self) -> Result<Vec<DraftSegment>, LiveError> {
        self.sink.take();
        self.worker.join().map_err(|_| LiveError::Panicked)?
    }
}

fn run(
    rx: mpsc::Receiver<Vec<f32>>,
    transcriber: Arc<Mutex<Transcriber>>,
    mut draft: DraftWriter,
    finals: Arc<Mutex<Vec<DraftSegment>>>,
    mut on_event: impl FnMut(LiveEvent),
) -> Result<Vec<DraftSegment>, LiveError> {
    let mut segmenter = Segmenter::new();
    let mut queue: VecDeque<Segment> = VecDeque::new();
    let mut open = true;
    let mut last_preview = Instant::now();

    loop {
        if open && queue.is_empty() {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(block) => queue.extend(segmenter.push(&block)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => open = false,
            }
        }
        // Catch up on everything queued, so we know whether finals are waiting.
        while open {
            match rx.try_recv() {
                Ok(block) => queue.extend(segmenter.push(&block)),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => open = false,
            }
        }

        // Finals always first; no final text is ever skipped.
        if let Some(segment) = queue.pop_front() {
            let text = transcriber.lock().expect("transcriber poisoned").transcribe(&segment.samples)?;
            if !text.is_empty() {
                let final_segment = DraftSegment { t_ms: segment.start_ms, text };
                draft.append(&final_segment)?;
                finals.lock().expect("finals poisoned").push(final_segment.clone());
                on_event(LiveEvent::Final(final_segment));
            }
            last_preview = Instant::now();
            continue;
        }
        if !open {
            match segmenter.flush() {
                Some(last) => {
                    queue.push_back(last);
                    continue;
                }
                None => break,
            }
        }

        // Previews only when no final is waiting.
        let tail = segmenter.tail();
        if last_preview.elapsed() >= PREVIEW_EVERY && tail.len() >= PREVIEW_MIN {
            let window = &tail[tail.len().saturating_sub(PREVIEW_WINDOW)..];
            let text = transcriber.lock().expect("transcriber poisoned").preview(window)?;
            on_event(LiveEvent::Preview(text));
            last_preview = Instant::now();
        }
    }

    draft.finish()?;
    Ok(std::mem::take(&mut *finals.lock().expect("finals poisoned")))
}
