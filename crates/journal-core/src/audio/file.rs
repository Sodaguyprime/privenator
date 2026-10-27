//! The encrypted audio file: Ogg Opus (RFC 7845), 16 kHz mono, ~24 kbps,
//! split into sealed-stream chunks of about one second. Concatenating the
//! decrypted chunks gives a standard `.opus` file.

use std::io::Cursor;
use std::path::Path;

use ogg::{PacketReader, PacketWriteEndInfo, PacketWriter};
use opus::{Application, Bitrate, Channels, Decoder, Encoder};
use zeroize::Zeroizing;

use super::SAMPLE_RATE;
use crate::stream::{self, StreamError, StreamWriter};
use crate::vault::{RecordingPublicKey, Session};

const FRAME: usize = SAMPLE_RATE as usize / 50; // 20 ms
const FRAMES_PER_CHUNK: u32 = 50; // 1 s
const BITRATE: i32 = 24_000;
/// Ogg Opus granule positions always count 48 kHz samples.
const GRANULE_PER_SAMPLE: u64 = 48_000 / SAMPLE_RATE as u64;
const SERIAL: u32 = 0x5056_4a41; // "PVJA"; one logical stream per file

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error(transparent)]
    Stream(#[from] StreamError),
    #[error("opus: {0}")]
    Opus(#[from] opus::Error),
    #[error("not a valid Ogg Opus stream: {0}")]
    Format(String),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

/// Encodes 16 kHz mono samples to Opus and appends them to a sealed stream.
/// Needs only the recording public key, so it keeps working while locked.
pub struct AudioWriter {
    stream: StreamWriter,
    encoder: Encoder,
    ogg: PacketWriter<'static, Vec<u8>>,
    pending: Vec<f32>,
    pre_skip: u64,
    samples: u64,
    frames_in_chunk: u32,
}

impl AudioWriter {
    pub fn create(path: &Path, recipient: &RecordingPublicKey) -> Result<Self, AudioError> {
        let mut encoder = Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip)?;
        encoder.set_bitrate(Bitrate::Bits(BITRATE))?;
        let pre_skip = encoder.get_lookahead()? as u64 * GRANULE_PER_SAMPLE;

        let mut writer = Self {
            stream: StreamWriter::create(path, recipient)?,
            encoder,
            ogg: PacketWriter::new(Vec::new()),
            pending: Vec::with_capacity(FRAME),
            pre_skip,
            samples: 0,
            frames_in_chunk: 0,
        };
        writer.ogg.write_packet(opus_head(pre_skip as u16), SERIAL, PacketWriteEndInfo::EndPage, 0)?;
        writer.ogg.write_packet(opus_tags(), SERIAL, PacketWriteEndInfo::EndPage, 0)?;
        writer.flush_chunk()?;
        Ok(writer)
    }

    /// Appends 16 kHz mono samples. A sealed chunk is written about once a second.
    pub fn push(&mut self, mut samples: &[f32]) -> Result<(), AudioError> {
        while !samples.is_empty() {
            let take = (FRAME - self.pending.len()).min(samples.len());
            self.pending.extend_from_slice(&samples[..take]);
            samples = &samples[take..];
            if self.pending.len() == FRAME {
                self.encode_pending(FRAME, false)?;
            }
        }
        Ok(())
    }

    /// Encodes what's left and marks the stream complete.
    pub fn finish(mut self) -> Result<(), AudioError> {
        // The last packet always exists and ends the Ogg stream; its granule
        // position tells players to drop the zero padding.
        let real = self.pending.len();
        self.pending.resize(FRAME, 0.0);
        self.encode_pending(real, true)?;
        self.stream.finish()?;
        Ok(())
    }

    fn encode_pending(&mut self, real_samples: usize, last: bool) -> Result<(), AudioError> {
        let packet = self.encoder.encode_vec_float(&self.pending, 4000)?;
        self.pending.clear();
        self.samples += real_samples as u64;
        self.frames_in_chunk += 1;
        let end = if last {
            PacketWriteEndInfo::EndStream
        } else if self.frames_in_chunk >= FRAMES_PER_CHUNK {
            PacketWriteEndInfo::EndPage
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        let flush = !matches!(end, PacketWriteEndInfo::NormalPacket);
        self.ogg.write_packet(packet, SERIAL, end, self.pre_skip + self.samples * GRANULE_PER_SAMPLE)?;
        if flush {
            self.flush_chunk()?;
        }
        Ok(())
    }

    fn flush_chunk(&mut self) -> Result<(), AudioError> {
        let bytes = std::mem::take(self.ogg.inner_mut());
        self.stream.append(&bytes)?;
        self.frames_in_chunk = 0;
        Ok(())
    }
}

fn opus_head(pre_skip: u16) -> Vec<u8> {
    let mut h = b"OpusHead".to_vec();
    h.push(1); // version
    h.push(1); // channels
    h.extend_from_slice(&pre_skip.to_le_bytes());
    h.extend_from_slice(&SAMPLE_RATE.to_le_bytes()); // original input rate
    h.extend_from_slice(&0i16.to_le_bytes()); // output gain
    h.push(0); // channel mapping family
    h
}

fn opus_tags() -> Vec<u8> {
    let vendor = b"privenator";
    let mut t = b"OpusTags".to_vec();
    t.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    t.extend_from_slice(vendor);
    t.extend_from_slice(&0u32.to_le_bytes()); // no comments
    t
}

/// A decrypted audio file: a complete `.opus` byte stream, held in memory only.
pub struct OggAudio {
    pub bytes: Zeroizing<Vec<u8>>,
    /// False for a recording cut short by a crash; what was saved still plays.
    pub finished: bool,
}

pub fn read_ogg(path: &Path, session: &Session) -> Result<OggAudio, AudioError> {
    let contents = stream::read(path, session)?;
    let mut bytes = Zeroizing::new(Vec::with_capacity(contents.chunks.iter().map(|c| c.len()).sum()));
    for chunk in &contents.chunks {
        bytes.extend_from_slice(chunk);
    }
    Ok(OggAudio { bytes, finished: contents.finished })
}

/// Decodes an Ogg Opus stream written by `AudioWriter` to 16 kHz mono samples.
pub fn decode(ogg: &[u8]) -> Result<Vec<f32>, AudioError> {
    let format = |e: &dyn std::fmt::Display| AudioError::Format(e.to_string());
    let mut reader = PacketReader::new(Cursor::new(ogg));

    let head = reader.read_packet().map_err(|e| format(&e))?.ok_or_else(|| format(&"empty"))?;
    if head.data.len() < 19 || &head.data[..8] != b"OpusHead" || head.data[9] != 1 {
        return Err(format(&"missing mono OpusHead"));
    }
    let pre_skip = u16::from_le_bytes([head.data[10], head.data[11]]) as u64;
    reader.read_packet().map_err(|e| format(&e))?.ok_or_else(|| format(&"missing OpusTags"))?;

    let mut decoder = Decoder::new(SAMPLE_RATE, Channels::Mono)?;
    let mut pcm = Vec::new();
    let mut buf = vec![0f32; SAMPLE_RATE as usize * 120 / 1000]; // max Opus packet: 120 ms
    let mut end_granule = None;
    while let Some(packet) = reader.read_packet().map_err(|e| format(&e))? {
        let n = decoder.decode_float(&packet.data, &mut buf, false)?;
        pcm.extend_from_slice(&buf[..n]);
        end_granule = Some(packet.absgp_page());
    }

    let skip = (pre_skip / GRANULE_PER_SAMPLE) as usize;
    if let Some(end) = end_granule {
        pcm.truncate((end.saturating_sub(pre_skip) / GRANULE_PER_SAMPLE) as usize + skip);
    }
    pcm.drain(..skip.min(pcm.len()));
    Ok(pcm)
}
