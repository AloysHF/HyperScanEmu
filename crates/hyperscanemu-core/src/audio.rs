use std::collections::VecDeque;

use crate::CPU_CLOCK_HZ;

pub(crate) const DAC_BUFFER_START: u32 = 0x0807_0054;
pub(crate) const DAC_CLOCK_CONFIG: u32 = 0x0821_003c;
pub(crate) const DAC_MODE_CONTROL2: u32 = 0x0805_1034;
pub(crate) const DAC_OUTPUT_LEFT: u32 = 0x0805_1040;
pub(crate) const DAC_OUTPUT_RIGHT: u32 = 0x0805_1044;
pub(crate) const DAC_SAMPLE_CLOCK: u32 = 0x0805_1064;
pub(crate) const DAC_FIFO_BASE_LOW: u32 = 0x0805_1080;
pub(crate) const DAC_FIFO_BASE_HIGH: u32 = 0x0805_1084;
pub(crate) const DAC_INTERRUPT_STATUS: u32 = 0x0805_1088;
pub(crate) const DAC_MODE_CONTROL1: u32 = 0x0805_1474;

const DAC_CLOCK_HZ: u64 = 54_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AudioDmaRequest {
    pub left_address: u32,
    pub right_address: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DacFifo {
    buffer_start: u32,
    clock_config: u32,
    mode_control1: u32,
    mode_control2: u32,
    sample_clock: u32,
    fifo_base_low: u32,
    fifo_base_high: u32,
    interrupt_control: u32,
    output_left: u16,
    output_right: u16,
    position: u32,
    phase: u64,
    dma_requests: VecDeque<AudioDmaRequest>,
    output_samples: Vec<i16>,
}

impl DacFifo {
    pub(crate) fn read(&self, address: u32) -> Option<u32> {
        match address {
            DAC_BUFFER_START => Some(self.buffer_start),
            DAC_CLOCK_CONFIG => Some(self.clock_config),
            DAC_MODE_CONTROL1 => Some(self.mode_control1),
            DAC_MODE_CONTROL2 => Some(self.mode_control2),
            DAC_OUTPUT_LEFT => Some(u32::from(self.output_left)),
            DAC_OUTPUT_RIGHT => Some(u32::from(self.output_right)),
            DAC_SAMPLE_CLOCK => Some(self.sample_clock),
            DAC_FIFO_BASE_LOW => Some(self.fifo_base_low),
            DAC_FIFO_BASE_HIGH => Some(self.fifo_base_high),
            DAC_INTERRUPT_STATUS => Some(self.interrupt_control),
            _ => None,
        }
    }

    pub(crate) fn write(&mut self, address: u32, value: u32) -> Option<()> {
        match address {
            DAC_BUFFER_START => {
                self.buffer_start = value;
                self.position = value % self.buffer_size();
            }
            DAC_CLOCK_CONFIG => self.clock_config = value,
            DAC_MODE_CONTROL1 => self.mode_control1 = value,
            DAC_MODE_CONTROL2 => self.mode_control2 = value,
            DAC_OUTPUT_LEFT => self.output_left = value as u16,
            DAC_OUTPUT_RIGHT => self.output_right = value as u16,
            DAC_SAMPLE_CLOCK => self.sample_clock = value,
            DAC_FIFO_BASE_LOW => self.fifo_base_low = value,
            DAC_FIFO_BASE_HIGH => self.fifo_base_high = value,
            DAC_INTERRUPT_STATUS => {
                let pending = self.interrupt_control & 0x8000;
                self.interrupt_control = (value & 0x7fff) | pending;
                if value & 0x8000 != 0 {
                    self.interrupt_control &= !0x8000;
                }
                self.position %= self.buffer_size();
            }
            _ => return None,
        }
        Some(())
    }

    pub(crate) fn tick(&mut self, cpu_cycles: u64) -> bool {
        if !self.enabled() {
            return false;
        }

        let divider = u64::from(self.sample_clock) + 1;
        self.phase = self
            .phase
            .saturating_add(cpu_cycles.saturating_mul(DAC_CLOCK_HZ));
        let threshold = CPU_CLOCK_HZ.saturating_mul(divider);
        let mut raised = false;
        while self.phase >= threshold {
            self.phase -= threshold;
            raised |= self.schedule_sample();
        }
        raised
    }

    pub(crate) fn take_dma_request(&mut self) -> Option<AudioDmaRequest> {
        self.dma_requests.pop_front()
    }

    pub(crate) fn complete_dma(&mut self, left: u16, right: u16) {
        self.output_left = left;
        self.output_right = right;
        self.output_samples.push((left ^ 0x8000) as i16);
        self.output_samples.push((right ^ 0x8000) as i16);
    }

    pub(crate) fn drain_samples(&mut self, destination: &mut Vec<i16>) {
        destination.clear();
        destination.append(&mut self.output_samples);
    }

    pub(crate) fn interrupt_pending(&self) -> bool {
        self.interrupt_control & 0xc000 == 0xc000
    }

    fn schedule_sample(&mut self) -> bool {
        let base = ((self.fifo_base_high & 0xffff) << 16) | (self.fifo_base_low & 0xffff);
        let stereo = self.interrupt_control & 4 != 0;
        let left_address = base.wrapping_add(self.position);
        let right_address = if stereo {
            left_address.wrapping_add(2)
        } else {
            left_address
        };
        self.dma_requests.push_back(AudioDmaRequest {
            left_address,
            right_address,
        });

        let stride = if stereo { 4 } else { 2 };
        let previous = self.position;
        self.position = (self.position + stride) % self.buffer_size();
        let half = self.buffer_size() / 2;
        let boundary = (previous < half && self.position >= half) || self.position < previous;
        if boundary {
            self.interrupt_control |= 0x8000;
        }
        boundary && self.interrupt_control & 0x4000 != 0
    }

    fn enabled(&self) -> bool {
        self.clock_config & 3 == 3
            && self.mode_control1 & 3 == 0
            && self.mode_control2 & 0x1000 != 0
            && self.mode_control2 & 3 != 0
    }

    fn buffer_size(&self) -> u32 {
        1024 << (self.interrupt_control & 3)
    }
}

/// Pending requests are transient, so the state keeps a fixed number of slots
/// and drops anything beyond them instead of making the state size vary.
const AUDIO_DMA_QUEUE_SLOTS: usize = 8;

impl AudioDmaRequest {
    pub(crate) const STATE_SIZE: usize = 8;

    fn save_state(&self, writer: &mut crate::state::StateWriter) {
        writer.u32(self.left_address);
        writer.u32(self.right_address);
    }

    fn load_state(reader: &mut crate::state::StateReader) -> Result<Self, crate::EmulatorError> {
        Ok(Self {
            left_address: reader.u32()?,
            right_address: reader.u32()?,
        })
    }
}

impl DacFifo {
    pub(crate) const STATE_SIZE: usize =
        8 * 4 + 2 * 2 + 4 + 8 + 1 + AUDIO_DMA_QUEUE_SLOTS * AudioDmaRequest::STATE_SIZE;

    pub(crate) fn save_state(&self, writer: &mut crate::state::StateWriter) {
        writer.u32(self.buffer_start);
        writer.u32(self.clock_config);
        writer.u32(self.mode_control1);
        writer.u32(self.mode_control2);
        writer.u32(self.sample_clock);
        writer.u32(self.fifo_base_low);
        writer.u32(self.fifo_base_high);
        writer.u32(self.interrupt_control);
        writer.u16(self.output_left);
        writer.u16(self.output_right);
        writer.u32(self.position);
        writer.u64(self.phase);
        let queued = self.dma_requests.len().min(AUDIO_DMA_QUEUE_SLOTS);
        writer.u8(queued as u8);
        for index in 0..AUDIO_DMA_QUEUE_SLOTS {
            match self.dma_requests.get(index) {
                Some(request) => request.save_state(writer),
                None => {
                    writer.u32(0);
                    writer.u32(0);
                }
            }
        }
    }

    pub(crate) fn load_state(
        &mut self,
        reader: &mut crate::state::StateReader,
    ) -> Result<(), crate::EmulatorError> {
        self.buffer_start = reader.u32()?;
        self.clock_config = reader.u32()?;
        self.mode_control1 = reader.u32()?;
        self.mode_control2 = reader.u32()?;
        self.sample_clock = reader.u32()?;
        self.fifo_base_low = reader.u32()?;
        self.fifo_base_high = reader.u32()?;
        self.interrupt_control = reader.u32()?;
        self.output_left = reader.u16()?;
        self.output_right = reader.u16()?;
        self.position = reader.u32()?;
        self.phase = reader.u64()?;
        let queued = usize::from(reader.u8()?).min(AUDIO_DMA_QUEUE_SLOTS);
        self.dma_requests.clear();
        for index in 0..AUDIO_DMA_QUEUE_SLOTS {
            let request = AudioDmaRequest::load_state(reader)?;
            if index < queued {
                self.dma_requests.push_back(request);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enable_stereo(dac: &mut DacFifo) {
        dac.write(DAC_CLOCK_CONFIG, 3).unwrap();
        dac.write(DAC_MODE_CONTROL1, !3).unwrap();
        dac.write(DAC_FIFO_BASE_LOW, 0x2000).unwrap();
        dac.write(DAC_FIFO_BASE_HIGH, 0).unwrap();
        dac.write(DAC_SAMPLE_CLOCK, 1_223).unwrap();
        dac.write(DAC_INTERRUPT_STATUS, 0x4007).unwrap();
        dac.write(DAC_MODE_CONTROL2, 0x1003).unwrap();
    }

    #[test]
    fn sample_clock_schedules_interleaved_stereo_reads() {
        let mut dac = DacFifo::default();
        enable_stereo(&mut dac);

        dac.tick(CPU_CLOCK_HZ * 1_224 / DAC_CLOCK_HZ);
        let request = dac.take_dma_request().unwrap();

        assert_eq!(request.left_address, 0x2000);
        assert_eq!(request.right_address, 0x2002);
    }

    #[test]
    fn half_buffer_sets_and_acknowledges_interrupt() {
        let mut dac = DacFifo::default();
        enable_stereo(&mut dac);
        dac.position = dac.buffer_size() / 2 - 4;

        assert!(dac.schedule_sample());
        assert!(dac.interrupt_pending());
        dac.write(DAC_INTERRUPT_STATUS, 0xc007).unwrap();

        assert!(!dac.interrupt_pending());
        assert_eq!(dac.read(DAC_INTERRUPT_STATUS).unwrap(), 0x4007);
    }

    #[test]
    fn unsigned_pcm_is_converted_to_signed_stereo() {
        let mut dac = DacFifo::default();
        dac.complete_dma(0, u16::MAX);
        let mut samples = Vec::new();
        dac.drain_samples(&mut samples);

        assert_eq!(samples, [i16::MIN, i16::MAX]);
    }
}
