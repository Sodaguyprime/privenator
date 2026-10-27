//! Seam: splitting a live 16 kHz stream into segments at natural pauses
//! (docs/SPEC.md "Recording and transcription", step 3).

use std::f32::consts::TAU;

use journal_core::live::Segmenter;

const RATE: usize = 16_000;

/// Stand-in for speech: a loud tone. The segmenter only looks at energy.
fn voice(seconds: f32) -> Vec<f32> {
    (0..(RATE as f32 * seconds) as usize).map(|i| 0.3 * (TAU * 220.0 * i as f32 / RATE as f32).sin()).collect()
}

/// Room noise, far below speech.
fn quiet(seconds: f32) -> Vec<f32> {
    (0..(RATE as f32 * seconds) as usize).map(|i| if i % 2 == 0 { 0.001 } else { -0.001 }).collect()
}

fn feed(segmenter: &mut Segmenter, audio: &[f32]) -> Vec<journal_core::live::Segment> {
    // Mic blocks are ~10 ms.
    audio.chunks(160).flat_map(|block| segmenter.push(block)).collect()
}

#[test]
fn a_pause_ends_a_segment() {
    let mut s = Segmenter::new();
    let audio = [voice(2.0), quiet(1.0), voice(2.0), quiet(1.0)].concat();

    let segments = feed(&mut s, &audio);

    assert_eq!(segments.len(), 2);
    assert!(segments[0].start_ms < 300, "first starts at {} ms", segments[0].start_ms);
    assert!((2_600..3_100).contains(&segments[1].start_ms), "second starts at {} ms", segments[1].start_ms);
    for seg in &segments {
        let secs = seg.samples.len() as f32 / RATE as f32;
        assert!((2.0..3.0).contains(&secs), "segment of {secs} s");
    }
}

#[test]
fn long_speech_is_cut_at_most_every_25_seconds_without_losing_audio() {
    let mut s = Segmenter::new();
    let mut segments = feed(&mut s, &voice(60.0));
    segments.extend(s.flush());

    assert!(segments.iter().all(|seg| seg.samples.len() <= 25 * RATE));
    let total: usize = segments.iter().map(|seg| seg.samples.len()).sum();
    assert_eq!(total, 60 * RATE);
}

#[test]
fn silence_produces_nothing_to_transcribe() {
    let mut s = Segmenter::new();
    let mut segments = feed(&mut s, &quiet(30.0));
    segments.extend(s.flush());
    assert!(segments.is_empty());
}

#[test]
fn the_unfinished_tail_is_available_for_previews_and_flushed_at_stop() {
    let mut s = Segmenter::new();
    assert!(feed(&mut s, &voice(1.5)).is_empty());
    assert!((1.4..1.6).contains(&(s.tail().len() as f32 / RATE as f32)));

    let last = s.flush().expect("speech in progress");
    assert!(last.start_ms < 100);
    assert!(s.tail().is_empty());
}
