//! Seams: converting device audio to 16 kHz mono, and the encrypted Opus audio
//! file (written while locked, read back after unlock).

use std::f32::consts::TAU;

use journal_core::audio::{self, AudioWriter, To16kMono};
use journal_core::vault::{KdfParams, Vault};
use secrecy::SecretString;

const FAST: KdfParams = KdfParams { m_kib: 1024, t: 1, p: 1 };

fn pw() -> SecretString {
    SecretString::from("correct horse battery staple".to_owned())
}

fn sine(hz: f32, rate: u32, channels: usize, seconds: f32) -> Vec<f32> {
    let frames = (rate as f32 * seconds) as usize;
    (0..frames)
        .flat_map(|i| std::iter::repeat_n(0.5 * (TAU * hz * i as f32 / rate as f32).sin(), channels))
        .collect()
}

/// Rising zero crossings per second, i.e. the dominant frequency of a clean tone.
fn frequency(samples: &[f32], rate: u32) -> f32 {
    let rising = samples.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    rising as f32 * rate as f32 / samples.len() as f32
}

#[test]
fn device_audio_becomes_16k_mono_at_the_same_pitch() {
    for (rate, channels) in [(48_000, 2), (44_100, 1), (16_000, 1)] {
        let input = sine(440.0, rate, channels, 2.0);
        let mut convert = To16kMono::new(rate, channels).unwrap();
        let mut out = Vec::new();
        // Device callbacks come in odd sizes.
        for buffer in input.chunks(channels * 317) {
            convert.push(buffer, &mut out);
        }

        assert!((30_000..=32_000).contains(&out.len()), "{rate} Hz × {channels}: {} samples", out.len());
        let f = frequency(&out[4_000..28_000], 16_000);
        assert!((435.0..445.0).contains(&f), "{rate} Hz × {channels}: {f} Hz");
    }
}

#[test]
fn audio_recorded_while_locked_plays_back_after_unlock() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    drop(vault.create(&pw(), FAST).unwrap()); // create, then lock
    let path = dir.path().join("rec.opus.enc");

    let mut writer = AudioWriter::create(&path, &vault.recording_public_key().unwrap()).unwrap();
    for block in sine(440.0, 16_000, 1, 3.0).chunks(1_000) {
        writer.push(block).unwrap();
    }
    writer.finish().unwrap();

    let session = vault.unlock(&pw()).unwrap();
    let ogg = audio::read_ogg(&path, &session).unwrap();
    assert!(ogg.finished);
    assert!(ogg.bytes.starts_with(b"OggS"));
    assert!(ogg.bytes.len() < 16_000, "about 24 kbps; got {} bytes for 3 s", ogg.bytes.len());

    let pcm = audio::decode(&ogg.bytes).unwrap();
    assert!((47_000..=49_000).contains(&pcm.len()), "{} samples", pcm.len());
    let f = frequency(&pcm[8_000..40_000], 16_000);
    assert!((435.0..445.0).contains(&f), "{f} Hz");
}

#[test]
fn a_crash_loses_at_most_the_last_second() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::at(dir.path());
    drop(vault.create(&pw(), FAST).unwrap());
    let path = dir.path().join("rec.opus.enc");

    let mut writer = AudioWriter::create(&path, &vault.recording_public_key().unwrap()).unwrap();
    writer.push(&sine(440.0, 16_000, 1, 2.5)).unwrap();
    drop(writer); // the process dies: no finish()

    let session = vault.unlock(&pw()).unwrap();
    let ogg = audio::read_ogg(&path, &session).unwrap();
    assert!(!ogg.finished);
    let pcm = audio::decode(&ogg.bytes).unwrap();
    assert!((31_000..=40_000).contains(&pcm.len()), "{} samples survived", pcm.len());
}
