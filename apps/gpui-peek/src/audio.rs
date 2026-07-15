//! Mic level metering (cpal) and beep playback (rodio).

use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

// ---------------------------------------------------------------------------
// Mic meter
// ---------------------------------------------------------------------------

/// Written by the CoreAudio callback thread, read by a 20 Hz UI task.
pub struct MicShared {
    rms_bits: AtomicU32,
    pub callbacks: AtomicU64,
}

impl MicShared {
    pub fn new() -> Self {
        Self {
            rms_bits: AtomicU32::new(0),
            callbacks: AtomicU64::new(0),
        }
    }

    pub fn rms(&self) -> f32 {
        f32::from_bits(self.rms_bits.load(Ordering::Relaxed))
    }

    fn store(&self, rms: f32) {
        self.rms_bits.store(rms.to_bits(), Ordering::Relaxed);
        self.callbacks.fetch_add(1, Ordering::Relaxed);
    }
}

fn update_rms(shared: &MicShared, samples: impl Iterator<Item = f32>) {
    let (mut sum_sq, mut n) = (0.0f64, 0u32);
    for s in samples {
        sum_sq += (s as f64) * (s as f64);
        n += 1;
    }
    if n > 0 {
        shared.store((sum_sq / n as f64).sqrt() as f32);
    }
}

/// Opens the default input device. On macOS the first buffer callback is what
/// triggers the microphone TCC prompt; a denial does NOT error — CoreAudio
/// just delivers silence (all-zero buffers).
pub fn start_mic(shared: Arc<MicShared>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "no default input device".to_string())?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("default_input_config: {e}"))?;
    let sample_format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();
    let err_fn = |e| eprintln!("MIC stream error: {e}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _| update_rms(&shared, data.iter().copied()),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _| {
                update_rms(&shared, data.iter().map(|s| *s as f32 / 32768.0))
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            move |data: &[u16], _| {
                update_rms(&shared, data.iter().map(|s| (*s as f32 - 32768.0) / 32768.0))
            },
            err_fn,
            None,
        ),
        other => return Err(format!("unsupported input sample format {other:?}")),
    }
    .map_err(|e| format!("build_input_stream: {e}"))?;

    stream.play().map_err(|e| format!("stream.play: {e}"))?;
    Ok(stream)
}

// ---------------------------------------------------------------------------
// Beep
// ---------------------------------------------------------------------------

/// Lazily opens the default output sink on first beep and keeps it for the
/// life of the app (rodio 0.22: DeviceSinkBuilder -> MixerDeviceSink).
pub struct Beeper {
    sink: Option<rodio::stream::MixerDeviceSink>,
    pub beeps: u64,
}

impl Beeper {
    pub fn new() -> Self {
        Self {
            sink: None,
            beeps: 0,
        }
    }

    pub fn beep(&mut self) -> Result<(), String> {
        use rodio::source::{SineWave, Source};
        if self.sink.is_none() {
            self.sink = Some(
                rodio::DeviceSinkBuilder::open_default_sink()
                    .map_err(|e| format!("open_default_sink: {e}"))?,
            );
        }
        let sink = self.sink.as_ref().unwrap();
        let source = SineWave::new(880.0)
            .take_duration(Duration::from_millis(180))
            .amplify(0.25);
        sink.mixer().add(source);
        self.beeps += 1;
        Ok(())
    }
}
