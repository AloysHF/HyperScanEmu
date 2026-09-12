use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, OutputCallbackInfo, Sample, SampleFormat, SizedSample, Stream};

const INPUT_RATE: u32 = 44_100;

pub(crate) struct AudioOutput {
    _stream: Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
    output_rate: u32,
    output_channels: usize,
    volume: f32,
    max_queued_samples: usize,
}

impl AudioOutput {
    pub(crate) fn new(volume_percent: u16) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no default audio output device is available")?;
        let supported = device
            .default_output_config()
            .context("failed to query the default audio output configuration")?;
        let sample_format = supported.sample_format();
        let config = supported.config();
        let output_rate = config.sample_rate;
        let output_channels = usize::from(config.channels);
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let stream = match sample_format {
            SampleFormat::F32 => build_stream::<f32>(&device, &config, Arc::clone(&queue)),
            SampleFormat::I16 => build_stream::<i16>(&device, &config, Arc::clone(&queue)),
            SampleFormat::U16 => build_stream::<u16>(&device, &config, Arc::clone(&queue)),
            format => bail!("unsupported audio device sample format {format}"),
        }?;
        stream.play().context("failed to start audio output")?;

        Ok(Self {
            _stream: stream,
            queue,
            output_rate,
            output_channels,
            volume: f32::from(volume_percent) / 100.0,
            max_queued_samples: output_rate as usize * output_channels / 4,
        })
    }

    pub(crate) fn submit(&self, samples: &[i16]) {
        if samples.len() < 2 || self.output_channels == 0 || self.volume == 0.0 {
            return;
        }
        let input_frames = samples.len() / 2;
        let output_frames =
            input_frames as u64 * u64::from(self.output_rate) / u64::from(INPUT_RATE);
        let mut converted = Vec::with_capacity(output_frames as usize * self.output_channels);
        for output_frame in 0..output_frames as usize {
            let position =
                output_frame as f64 * f64::from(INPUT_RATE) / f64::from(self.output_rate);
            let first = (position.floor() as usize).min(input_frames - 1);
            let second = (first + 1).min(input_frames - 1);
            let fraction = (position - first as f64) as f32;
            let left = interpolate(samples[first * 2], samples[second * 2], fraction) * self.volume;
            let right = interpolate(samples[first * 2 + 1], samples[second * 2 + 1], fraction)
                * self.volume;
            if self.output_channels == 1 {
                converted.push((left + right) * 0.5);
            } else {
                converted.push(left);
                converted.push(right);
                converted.extend(std::iter::repeat_n(
                    (left + right) * 0.5,
                    self.output_channels - 2,
                ));
            }
        }

        if let Ok(mut queue) = self.queue.lock() {
            let overflow = queue
                .len()
                .saturating_add(converted.len())
                .saturating_sub(self.max_queued_samples);
            let drain_count = overflow.min(queue.len());
            queue.drain(..drain_count);
            queue.extend(converted);
        }
    }

    pub(crate) fn clear(&self) {
        if let Ok(mut queue) = self.queue.lock() {
            queue.clear();
        }
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: Arc<Mutex<VecDeque<f32>>>,
) -> Result<Stream>
where
    T: SizedSample + FromSample<f32>,
{
    device
        .build_output_stream(
            *config,
            move |output: &mut [T], _: &OutputCallbackInfo| write_output(output, &queue),
            |error| eprintln!("audio output error: {error}"),
            None,
        )
        .context("failed to create audio output stream")
}

fn write_output<T>(output: &mut [T], queue: &Arc<Mutex<VecDeque<f32>>>)
where
    T: Sample + FromSample<f32>,
{
    if let Ok(mut queue) = queue.lock() {
        for sample in output {
            *sample = T::from_sample(queue.pop_front().unwrap_or(0.0));
        }
    } else {
        output.fill(T::from_sample(0.0));
    }
}

fn interpolate(first: i16, second: i16, fraction: f32) -> f32 {
    let first = first as f32 / i16::MAX as f32;
    let second = second as f32 / i16::MAX as f32;
    first + (second - first) * fraction
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_normalizes_signed_pcm() {
        assert!((interpolate(0, i16::MAX, 0.5) - 0.5).abs() < f32::EPSILON);
    }
}
