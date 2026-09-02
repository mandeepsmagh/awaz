use awaz_core::AudioChunk;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, TrySendError, bounded};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no input device available")]
    NoInputDevice,
    #[error("audio backend error: {0}")]
    Backend(String),
    #[error("unsupported input sample format: {0}")]
    UnsupportedFormat(String),
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub device_name: Option<String>,
    pub queue_capacity: usize,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            queue_capacity: 64,
        }
    }
}

pub struct AudioCapture {
    _stream: cpal::Stream,
    receiver: Receiver<AudioChunk>,
    dropped_chunks: Arc<AtomicU64>,
    pub device_name: String,
    pub sample_rate: u32,
}

impl AudioCapture {
    pub fn start(config: CaptureConfig) -> Result<Self, AudioError> {
        let host = preferred_host();
        let device = select_device(&host, config.device_name.as_deref())?;
        let device_name = device_display_name(&device);
        let supported = device
            .default_input_config()
            .map_err(|e| AudioError::Backend(e.to_string()))?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels() as usize;
        let stream_config: cpal::StreamConfig = supported.clone().into();
        let (sender, receiver) = bounded(config.queue_capacity.max(4));
        let dropped_chunks = Arc::new(AtomicU64::new(0));

        macro_rules! build_stream {
            ($ty:ty) => {{
                let sender = sender.clone();
                let dropped = dropped_chunks.clone();
                device.build_input_stream(
                    stream_config,
                    move |data: &[$ty], _| {
                        let mono = downmix(data, channels);
                        if let Err(TrySendError::Full(_)) = sender.try_send(AudioChunk {
                            samples: mono,
                            sample_rate,
                        }) {
                            dropped.fetch_add(1, Ordering::Relaxed);
                        }
                    },
                    move |err| eprintln!("awaz audio stream error: {err}"),
                    None,
                )
            }};
        }

        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build_stream!(f32),
            cpal::SampleFormat::F64 => build_stream!(f64),
            cpal::SampleFormat::I8 => build_stream!(i8),
            cpal::SampleFormat::I16 => build_stream!(i16),
            cpal::SampleFormat::I24 => build_stream!(cpal::I24),
            cpal::SampleFormat::I32 => build_stream!(i32),
            cpal::SampleFormat::I64 => build_stream!(i64),
            cpal::SampleFormat::U8 => build_stream!(u8),
            cpal::SampleFormat::U16 => build_stream!(u16),
            cpal::SampleFormat::U24 => build_stream!(cpal::U24),
            cpal::SampleFormat::U32 => build_stream!(u32),
            cpal::SampleFormat::U64 => build_stream!(u64),
            other => return Err(AudioError::UnsupportedFormat(format!("{other:?}"))),
        }
        .map_err(|e| AudioError::Backend(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::Backend(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            receiver,
            dropped_chunks,
            device_name,
            sample_rate,
        })
    }

    pub fn receiver(&self) -> Receiver<AudioChunk> {
        self.receiver.clone()
    }

    pub fn dropped_chunks(&self) -> u64 {
        self.dropped_chunks.load(Ordering::Relaxed)
    }
}

pub fn list_input_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    let host = preferred_host();
    let default_name = host
        .default_input_device()
        .as_ref()
        .map(device_display_name);
    let devices = host
        .input_devices()
        .map_err(|e| AudioError::Backend(e.to_string()))?;
    let mut out = Vec::new();
    for device in devices {
        let name = device_display_name(&device);
        out.push(DeviceInfo {
            is_default: default_name.as_deref() == Some(name.as_str()),
            name,
        });
    }
    Ok(out)
}

fn preferred_host() -> cpal::Host {
    #[cfg(target_os = "linux")]
    {
        if let Ok(host) = cpal::host_from_id(cpal::HostId::PipeWire) {
            if host.default_input_device().is_some() {
                return host;
            }
        }
    }
    cpal::default_host()
}

fn select_device(host: &cpal::Host, requested: Option<&str>) -> Result<cpal::Device, AudioError> {
    if let Some(requested) = requested {
        let mut devices = host
            .input_devices()
            .map_err(|e| AudioError::Backend(e.to_string()))?;
        if let Some(device) = devices.find(|device| device_display_name(device) == requested) {
            return Ok(device);
        }
        return Err(AudioError::Backend(format!(
            "input device not found: {requested}"
        )));
    }
    host.default_input_device().ok_or(AudioError::NoInputDevice)
}

fn device_display_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|description| description.name().to_owned())
        .unwrap_or_else(|_| device.to_string())
}

fn downmix<T>(input: &[T], channels: usize) -> Vec<f32>
where
    T: Copy,
    f32: cpal::FromSample<T>,
{
    if channels <= 1 {
        return input
            .iter()
            .copied()
            .map(<f32 as cpal::FromSample<T>>::from_sample_)
            .collect();
    }

    let mut mono = Vec::with_capacity(input.len() / channels);
    for frame in input.chunks_exact(channels) {
        let sum: f32 = frame
            .iter()
            .copied()
            .map(<f32 as cpal::FromSample<T>>::from_sample_)
            .sum();
        mono.push(sum / channels as f32);
    }
    mono
}
