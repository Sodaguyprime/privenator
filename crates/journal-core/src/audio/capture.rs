//! Microphone capture through cpal (WASAPI, CoreAudio, ALSA/PipeWire, AAudio).
//! Hardware-bound, so it has no automated tests; see docs/ARCHITECTURE.md.

use std::sync::mpsc;
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use super::To16kMono;

#[derive(Debug, Clone, serde::Serialize)]
pub struct InputDevice {
    /// Stable across restarts; what the `mic_device` setting stores.
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("no microphone found")]
    NoDevice,
    #[error("microphone access is blocked")]
    PermissionDenied,
    #[error("microphone error: {0}")]
    Device(String),
}

impl From<cpal::Error> for CaptureError {
    fn from(e: cpal::Error) -> Self {
        match e.kind() {
            cpal::ErrorKind::PermissionDenied => Self::PermissionDenied,
            cpal::ErrorKind::DeviceNotAvailable => Self::NoDevice,
            _ => Self::Device(e.to_string()),
        }
    }
}

pub fn input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let default_id = host.default_input_device().and_then(|d| d.id().ok()).map(|id| id.to_string());
    let Ok(devices) = host.input_devices() else { return Vec::new() };
    devices
        .filter_map(|d| {
            let id = d.id().ok()?.to_string();
            let name = d.description().map(|desc| desc.name().to_owned()).unwrap_or_else(|_| id.clone());
            Some(InputDevice { is_default: Some(&id) == default_id.as_ref(), id, name })
        })
        .collect()
}

enum Control {
    Pause,
    Play,
}

/// An open microphone delivering 16 kHz mono blocks to a callback.
/// cpal streams are not `Send` on every platform, so the stream lives on a
/// thread of its own; dropping `Mic` closes it.
pub struct Mic {
    control: Option<mpsc::Sender<Control>>,
    thread: Option<JoinHandle<()>>,
}

impl Mic {
    /// `device_id` None, or one that no longer exists, means the system default.
    /// `on_audio` runs on the audio thread: keep it cheap, and never block or
    /// touch the disk in it.
    pub fn open(device_id: Option<&str>, on_audio: impl FnMut(&[f32]) + Send + 'static) -> Result<Self, CaptureError> {
        let device_id = device_id.map(str::to_owned);
        let (ready_tx, ready_rx) = mpsc::channel();
        let (control_tx, control_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("mic".into())
            .spawn(move || {
                let stream = match build_stream(device_id.as_deref(), on_audio) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                // Runs until `Mic` is dropped (the sender closes).
                for control in control_rx {
                    let result = match control {
                        Control::Pause => stream.pause(),
                        Control::Play => stream.play(),
                    };
                    if let Err(e) = result {
                        eprintln!("mic: {e}");
                    }
                }
            })
            .map_err(|e| CaptureError::Device(e.to_string()))?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { control: Some(control_tx), thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(CaptureError::Device("capture thread ended".into())),
        }
    }

    pub fn pause(&self) {
        self.send(Control::Pause);
    }

    pub fn resume(&self) {
        self.send(Control::Play);
    }

    fn send(&self, control: Control) {
        if let Some(tx) = &self.control {
            let _ = tx.send(control);
        }
    }
}

impl Drop for Mic {
    fn drop(&mut self) {
        self.control.take(); // ends the thread's loop, which drops the stream
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn build_stream(device_id: Option<&str>, on_audio: impl FnMut(&[f32]) + Send + 'static) -> Result<cpal::Stream, CaptureError> {
    let host = cpal::default_host();
    let device = device_id
        .and_then(|id| id.parse().ok())
        .and_then(|id| host.device_by_id(&id))
        .or_else(|| host.default_input_device())
        .ok_or(CaptureError::NoDevice)?;
    let config = device.default_input_config()?;
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let convert = To16kMono::new(config.sample_rate, config.channels as usize).map_err(|e| CaptureError::Device(e.to_string()))?;

    let stream = match format {
        SampleFormat::F32 => typed::<f32>(&device, config, convert, on_audio),
        SampleFormat::I16 => typed::<i16>(&device, config, convert, on_audio),
        SampleFormat::I32 => typed::<i32>(&device, config, convert, on_audio),
        SampleFormat::U16 => typed::<u16>(&device, config, convert, on_audio),
        other => return Err(CaptureError::Device(format!("unsupported sample format {other}"))),
    }?;
    stream.play()?;
    Ok(stream)
}

fn typed<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut convert: To16kMono,
    mut on_audio: impl FnMut(&[f32]) + Send + 'static,
) -> Result<cpal::Stream, CaptureError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let mut floats = Vec::new();
    let mut out = Vec::new();
    Ok(device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            floats.clear();
            floats.extend(data.iter().map(|&s| s.to_sample::<f32>()));
            out.clear();
            convert.push(&floats, &mut out);
            if !out.is_empty() {
                on_audio(&out);
            }
        },
        |e| eprintln!("mic stream: {e}"),
        None,
    )?)
}

/// Loudness of a block for the level meter: RMS mapped from -60..0 dBFS to 0..1.
pub fn level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    let db = 20.0 * rms.max(1e-6).log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}
