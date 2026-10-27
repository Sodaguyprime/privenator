use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use super::SAMPLE_RATE;

#[derive(Debug, thiserror::Error)]
#[error("unsupported input format: {0}")]
pub struct ConvertError(String);

/// Turns interleaved device audio (any rate, any channel count) into 16 kHz
/// mono. Channels are averaged; the rate is changed with an FFT resampler.
pub struct To16kMono {
    channels: usize,
    resampler: Option<Fft<f32>>,
    pending: Vec<f32>,
}

impl To16kMono {
    pub fn new(rate: u32, channels: usize) -> Result<Self, ConvertError> {
        if channels == 0 || rate == 0 {
            return Err(ConvertError(format!("{rate} Hz, {channels} channels")));
        }
        let resampler = (rate != SAMPLE_RATE)
            .then(|| {
                // 10 ms of input per call.
                Fft::new(rate as usize, SAMPLE_RATE as usize, (rate / 100) as usize, 1, FixedSync::Input)
                    .map_err(|e| ConvertError(e.to_string()))
            })
            .transpose()?;
        Ok(Self { channels, resampler, pending: Vec::new() })
    }

    /// Appends the converted audio for `interleaved` to `out`. Input that
    /// doesn't fill a whole resampler chunk is kept for the next call.
    pub fn push(&mut self, interleaved: &[f32], out: &mut Vec<f32>) {
        let mono = interleaved.chunks_exact(self.channels).map(|f| f.iter().sum::<f32>() / self.channels as f32);
        let Some(resampler) = &mut self.resampler else {
            out.extend(mono);
            return;
        };
        self.pending.extend(mono);
        let mut used = 0;
        loop {
            let need = resampler.input_frames_next();
            if self.pending.len() - used < need {
                break;
            }
            let input = InterleavedSlice::new(&self.pending[used..used + need], 1, need).expect("sized above");
            let output = resampler.process(&input, None).expect("buffers sized by the resampler");
            out.extend(output.take_data());
            used += need;
        }
        self.pending.drain(..used);
    }
}
