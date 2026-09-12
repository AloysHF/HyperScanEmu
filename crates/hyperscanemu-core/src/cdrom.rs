use std::collections::VecDeque;

use crate::EmulatorError;

const LEAD_IN_SECTORS: u32 = 7_500;
const LEAD_OUT_SECTORS: u32 = 6_750;
const COMMAND_TRACE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdCommandKind {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdCommandTrace {
    pub kind: CdCommandKind,
    pub address: u32,
    pub data: u32,
    pub pc: u32,
    pub link: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdServoState {
    pub current_sector: u32,
    pub seek_lba: u32,
    pub skip: u16,
    pub speed: u8,
    pub frame_found: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CdDmaRequest {
    pub lba: u32,
    pub buffer_start: u32,
    pub buffer_end: u32,
    pub buffer_pointer: u32,
    pub sector_size: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct CdServo {
    system_control: u32,
    address: u32,
    data: u32,
    buffer_start: u32,
    buffer_end: u32,
    buffer_pointer: u32,
    speed: u8,
    seek_minute: u8,
    seek_second: u8,
    seek_frame: u8,
    seek_lba: u32,
    sector_size: u32,
    dma_control: u32,
    dsp_data: u32,
    frame_found: bool,
    control0: u32,
    control1: u32,
    auxiliary_control: u32,
    skip: u16,
    dsp_registers: [u32; 16],
    dsp_memory: Box<[u32]>,
    disc_sectors: Option<u32>,
    current_sector: u32,
    total_sectors: u32,
    phase: u64,
    pending_dma: Option<CdDmaRequest>,
    noise: u32,
    command_trace: VecDeque<CdCommandTrace>,
    execution_pc: u32,
    execution_link: u32,
}

impl Default for CdServo {
    fn default() -> Self {
        Self {
            system_control: 0,
            address: 0,
            data: 0,
            buffer_start: 0,
            buffer_end: 0,
            buffer_pointer: 0,
            speed: 1,
            seek_minute: 0,
            seek_second: 0,
            seek_frame: 0,
            seek_lba: 0,
            sector_size: 0,
            dma_control: 0,
            dsp_data: 0,
            frame_found: false,
            control0: 0,
            control1: 0,
            auxiliary_control: 0,
            skip: 0,
            dsp_registers: [0; 16],
            dsp_memory: vec![0; 0x1_0000].into_boxed_slice(),
            disc_sectors: None,
            current_sector: 0,
            total_sectors: 0,
            phase: 0,
            pending_dma: None,
            noise: 0x2905_2006,
            command_trace: VecDeque::with_capacity(COMMAND_TRACE_CAPACITY),
            execution_pc: 0,
            execution_link: 0,
        }
    }
}

impl CdServo {
    pub(crate) fn set_disc(&mut self, sector_count: Option<u32>) {
        self.disc_sectors = sector_count;
        self.total_sectors = sector_count
            .map(|count| LEAD_IN_SECTORS + 150 + count + LEAD_OUT_SECTORS)
            .unwrap_or(0);
        self.current_sector = 0;
        self.pending_dma = None;
        self.phase = 0;
    }

    pub(crate) fn disc_sector_count(&self) -> Option<u32> {
        self.disc_sectors
    }

    pub(crate) fn read(&self, offset: u32) -> Option<u32> {
        match offset {
            0x00 => Some(self.system_control),
            0x08 => Some(self.data),
            0x0c => Some(0),
            0x40 => Some(self.control0),
            0x44 => Some(self.control1),
            0x48 => Some(u32::from(self.seek_minute)),
            0x4c => Some(u32::from(self.seek_second)),
            0x50 => Some(u32::from(self.seek_frame)),
            0x54 => Some(0),
            0x60 => Some(self.buffer_start),
            0x64 => Some(self.buffer_end),
            0x68 => Some(self.buffer_pointer),
            0x6c => Some(self.sector_size),
            0x70 => Some(self.dma_control),
            0x80 => Some(self.auxiliary_control),
            _ => None,
        }
    }

    pub(crate) fn write(&mut self, offset: u32, value: u32) -> Option<()> {
        match offset {
            0x00 => self.system_control = value,
            0x04 => self.address = value,
            0x08 => self.data = value,
            0x0c => {
                if value & 1 != 0 {
                    self.execute_read_command();
                } else {
                    self.execute_write_command();
                }
            }
            0x40 => self.control0 = value,
            0x44 => {
                self.control1 = value;
                self.seek_lba = bcd_to_binary(self.seek_minute) * 60 * 75
                    + bcd_to_binary(self.seek_second) * 75
                    + bcd_to_binary(self.seek_frame);
                self.current_sector = LEAD_IN_SECTORS + self.seek_lba;
                self.phase = 0;
            }
            0x48 => self.seek_minute = value as u8,
            0x4c => self.seek_second = value as u8,
            0x50 => self.seek_frame = value as u8,
            0x54 => {}
            0x60 => self.buffer_start = value,
            0x64 => self.buffer_end = value,
            0x68 => self.buffer_pointer = value,
            0x6c => self.sector_size = value,
            0x70 => self.dma_control = value,
            0x80 => self.auxiliary_control = value,
            _ => return None,
        }
        Some(())
    }

    pub(crate) fn tick(&mut self, cpu_cycles: u64) -> Result<(), EmulatorError> {
        if self.disc_sectors.is_none() || self.speed == 0 || self.pending_dma.is_some() {
            return Ok(());
        }
        self.phase = self
            .phase
            .saturating_add(cpu_cycles.saturating_mul(75 * u64::from(self.speed)));
        while self.phase >= crate::CPU_CLOCK_HZ {
            self.phase -= crate::CPU_CLOCK_HZ;
            self.advance_sector()?;
            if self.pending_dma.is_some() {
                break;
            }
        }
        Ok(())
    }

    pub(crate) fn take_dma_request(&mut self) -> Option<CdDmaRequest> {
        self.pending_dma.take()
    }

    pub(crate) fn complete_dma(&mut self, next_pointer: u32) {
        self.buffer_pointer = next_pointer;
        self.frame_found = true;
    }

    pub(crate) fn frame_found(&self) -> bool {
        self.frame_found
    }

    pub(crate) fn command_trace(&self) -> Vec<CdCommandTrace> {
        self.command_trace.iter().copied().collect()
    }

    pub(crate) fn set_execution_context(&mut self, pc: u32, link: u32) {
        self.execution_pc = pc;
        self.execution_link = link;
    }

    pub(crate) fn state(&self) -> CdServoState {
        CdServoState {
            current_sector: self.current_sector,
            seek_lba: self.seek_lba,
            skip: self.skip,
            speed: self.speed,
            frame_found: self.frame_found,
        }
    }

    fn advance_sector(&mut self) -> Result<(), EmulatorError> {
        if self.control1 & 4 == 0 && self.current_sector == LEAD_IN_SECTORS + self.seek_lba {
            if self.control0 & (1 << 15) != 0 {
                return Err(EmulatorError::UnsupportedCdAudio);
            }
            let size = self.sector_size as usize;
            if size > crate::RAW_SECTOR_SIZE {
                return Err(EmulatorError::InvalidCdSectorSize {
                    size,
                    maximum: crate::RAW_SECTOR_SIZE,
                });
            }
            if self.seek_lba >= 150 {
                self.pending_dma = Some(CdDmaRequest {
                    lba: self.seek_lba - 150,
                    buffer_start: self.buffer_start,
                    buffer_end: self.buffer_end,
                    buffer_pointer: self.buffer_pointer,
                    sector_size: size,
                });
            }
            self.seek_lba = self.seek_lba.saturating_add(1);
        }

        if self.current_sector < LEAD_IN_SECTORS + self.seek_lba
            && self.current_sector < self.total_sectors
        {
            self.current_sector += 1;
        }
        Ok(())
    }

    fn execute_read_command(&mut self) {
        if self.address == 0x083 {
            self.noise = self
                .noise
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
        }
        self.data = match self.address {
            0x079 => 0xe3,
            0x07b => 0x06,
            0x07c => self.dsp_data >> 8,
            0x07d => self.dsp_data & 0xff,
            0x082 => 0x04,
            0x083 => self.noise & 0xff,
            0x307 => u32::from(self.frame_found),
            0x340..=0x349 => u32::from(self.q_subchannel((self.address & 0x0f) as usize)),
            0x34c => 0x40,
            0x506 => 0x04,
            0x500..=0x509 => self.dsp_registers[(self.address & 0x0f) as usize],
            0x41f => 0xff,
            _ => 0,
        };
        self.record_command(CdCommandKind::Read);
    }

    fn execute_write_command(&mut self) {
        self.record_command(CdCommandKind::Write);
        match self.address {
            0x013 => {
                if self.data & 1 != 0 {
                    let amount = u32::from(self.skip);
                    self.current_sector = if self.data & 0x10 != 0 {
                        self.current_sector.saturating_sub(amount)
                    } else {
                        self.current_sector.saturating_add(amount)
                    };
                    self.current_sector = self
                        .current_sector
                        .min(self.total_sectors.saturating_sub(1));
                }
            }
            0x020 => self.speed = 1 << (self.data & 3),
            0x030 => {
                self.dsp_data = if self.data == 0x13 {
                    8
                } else if self.disc_sectors.is_some() {
                    100
                } else {
                    0
                };
            }
            0x032 => self.dsp_data = 0x0102,
            0x19a => self.skip = (self.skip & 0x00ff) | ((self.data as u16) << 8),
            0x29a => self.skip = (self.skip & 0xff00) | self.data as u16,
            0x307 => self.frame_found = false,
            0x505 => self.access_dsp_memory(),
            0x500..=0x509 => self.dsp_registers[(self.address & 0x0f) as usize] = self.data,
            _ => {}
        }
    }

    fn access_dsp_memory(&mut self) {
        let address = ((self.dsp_registers[0] << 8) | self.dsp_registers[1]) as u16;
        let index = usize::from(address);
        match self.data & 0x0f {
            2 => {
                self.dsp_memory[index] = (self.dsp_registers[2] << 16)
                    | (self.dsp_registers[3] << 8)
                    | self.dsp_registers[4];
            }
            3 => {
                let value = self.dsp_memory[index];
                self.dsp_registers[7] = (value >> 16) & 0xff;
                self.dsp_registers[8] = (value >> 8) & 0xff;
                self.dsp_registers[9] = value & 0xff;
            }
            _ => {}
        }
    }

    fn q_subchannel(&self, index: usize) -> u8 {
        if self.current_sector < LEAD_IN_SECTORS {
            return self.lead_in_q_subchannel(index);
        }
        let relative = self.current_sector.saturating_sub(LEAD_IN_SECTORS);
        let (track, track_index, track_frame) = if relative < 150 {
            (1, 0, relative)
        } else {
            (1, 1, relative - 150)
        };
        let values = [
            0x41,
            binary_to_bcd(track),
            track_index,
            binary_to_bcd(track_frame / (60 * 75)),
            binary_to_bcd((track_frame / 75) % 60),
            binary_to_bcd(track_frame % 75),
            0,
            binary_to_bcd(relative / (60 * 75)),
            binary_to_bcd((relative / 75) % 60),
            binary_to_bcd(relative % 75),
        ];
        values.get(index).copied().unwrap_or(0)
    }

    fn lead_in_q_subchannel(&self, index: usize) -> u8 {
        let entry = self.current_sector % 4;
        let point = [0xa0, 0xa1, 0xa2, 0x01][entry as usize];
        let program_end = self.disc_sectors.unwrap_or(0).saturating_add(150);
        let absolute = match entry {
            0 | 1 => 1 << 16,
            2 => program_end,
            _ => 150,
        };
        let values = [
            0x41,
            0,
            point,
            binary_to_bcd(self.current_sector / (60 * 75)),
            binary_to_bcd((self.current_sector / 75) % 60),
            binary_to_bcd(self.current_sector % 75),
            0,
            binary_to_bcd(absolute / (60 * 75)),
            binary_to_bcd((absolute / 75) % 60),
            binary_to_bcd(absolute % 75),
        ];
        values.get(index).copied().unwrap_or(0)
    }

    fn record_command(&mut self, kind: CdCommandKind) {
        let event = CdCommandTrace {
            kind,
            address: self.address,
            data: self.data,
            pc: self.execution_pc,
            link: self.execution_link,
        };
        if self.command_trace.back() == Some(&event) {
            return;
        }
        if self.command_trace.len() == COMMAND_TRACE_CAPACITY {
            self.command_trace.pop_front();
        }
        self.command_trace.push_back(event);
    }
}

fn bcd_to_binary(value: u8) -> u32 {
    u32::from(value >> 4) * 10 + u32::from(value & 0x0f)
}

fn binary_to_bcd(value: u32) -> u8 {
    (((value / 10) << 4) | (value % 10)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_dsp(servo: &mut CdServo, address: u32, data: u32) {
        servo.write(0x04, address).unwrap();
        servo.write(0x08, data).unwrap();
        servo.write(0x0c, 0).unwrap();
    }

    fn read_dsp(servo: &mut CdServo, address: u32) -> u32 {
        servo.write(0x04, address).unwrap();
        servo.write(0x0c, 1).unwrap();
        servo.read(0x08).unwrap()
    }

    #[test]
    fn disc_identification_and_dsp_version_are_visible() {
        let mut servo = CdServo::default();
        servo.set_disc(Some(100));

        write_dsp(&mut servo, 0x030, 0);
        assert_eq!(read_dsp(&mut servo, 0x07c), 0);
        assert_eq!(read_dsp(&mut servo, 0x07d), 100);
        write_dsp(&mut servo, 0x032, 0);
        assert_eq!(read_dsp(&mut servo, 0x07c), 1);
        assert_eq!(read_dsp(&mut servo, 0x07d), 2);
    }

    #[test]
    fn auxiliary_control_register_round_trips() {
        let mut servo = CdServo::default();

        servo.write(0x80, 3).unwrap();

        assert_eq!(servo.read(0x80), Some(3));
    }

    #[test]
    fn analog_status_noise_is_deterministic_and_changes() {
        let mut first = CdServo::default();
        let mut second = CdServo::default();

        let first_sample = read_dsp(&mut first, 0x083);
        let next_sample = read_dsp(&mut first, 0x083);

        assert_ne!(first_sample, next_sample);
        assert_eq!(first_sample, read_dsp(&mut second, 0x083));
        assert_eq!(next_sample, read_dsp(&mut second, 0x083));
    }

    #[test]
    fn command_trace_deduplicates_repeated_polls() {
        let mut servo = CdServo::default();

        assert_eq!(read_dsp(&mut servo, 0x079), 0xe3);
        assert_eq!(read_dsp(&mut servo, 0x079), 0xe3);
        write_dsp(&mut servo, 0x020, 2);

        assert_eq!(
            servo.command_trace(),
            [
                CdCommandTrace {
                    kind: CdCommandKind::Read,
                    address: 0x079,
                    data: 0xe3,
                    pc: 0,
                    link: 0,
                },
                CdCommandTrace {
                    kind: CdCommandKind::Write,
                    address: 0x020,
                    data: 2,
                    pc: 0,
                    link: 0,
                },
            ]
        );
    }

    #[test]
    fn lead_in_q_subchannel_cycles_through_toc_entries() {
        let mut servo = CdServo::default();
        servo.set_disc(Some(20_000));

        let points = (0..4)
            .map(|_| {
                let point = read_dsp(&mut servo, 0x342);
                servo.current_sector += 1;
                point
            })
            .collect::<Vec<_>>();

        assert_eq!(points, [0xa0, 0xa1, 0xa2, 0x01]);
        servo.current_sector = 2;
        assert_eq!(read_dsp(&mut servo, 0x347), 0x04);
        assert_eq!(read_dsp(&mut servo, 0x348), 0x28);
        assert_eq!(read_dsp(&mut servo, 0x349), 0x50);
    }

    #[test]
    fn servo_state_reports_seek_diagnostics() {
        let mut servo = CdServo::default();
        servo.set_disc(Some(100));
        servo.skip = 127;
        servo.current_sector = 321;

        assert_eq!(
            servo.state(),
            CdServoState {
                current_sector: 321,
                seek_lba: 0,
                skip: 127,
                speed: 1,
                frame_found: false,
            }
        );
    }

    #[test]
    fn sector_transfer_is_scheduled_at_selected_speed() {
        let mut servo = CdServo::default();
        servo.set_disc(Some(100));
        servo.write(0x48, 0).unwrap();
        servo.write(0x4c, 2).unwrap();
        servo.write(0x50, 0).unwrap();
        servo.write(0x60, 0x100).unwrap();
        servo.write(0x64, 0x9ff).unwrap();
        servo.write(0x68, 0x100).unwrap();
        servo.write(0x6c, 2_352).unwrap();
        servo.write(0x44, 0).unwrap();

        servo.tick(crate::CPU_CLOCK_HZ / 75 - 1).unwrap();
        assert!(servo.take_dma_request().is_none());
        servo.tick(1).unwrap();
        let request = servo.take_dma_request().unwrap();

        assert_eq!(request.lba, 0);
        assert_eq!(request.buffer_pointer, 0x100);
        assert_eq!(request.sector_size, 2_352);
    }

    #[test]
    fn dsp_memory_round_trips_three_byte_words() {
        let mut servo = CdServo::default();
        for (address, data) in [
            (0x500, 0x12),
            (0x501, 0x34),
            (0x502, 0xaa),
            (0x503, 0xbb),
            (0x504, 0xcc),
        ] {
            write_dsp(&mut servo, address, data);
        }
        write_dsp(&mut servo, 0x505, 2);
        write_dsp(&mut servo, 0x505, 3);

        assert_eq!(read_dsp(&mut servo, 0x507), 0xaa);
        assert_eq!(read_dsp(&mut servo, 0x508), 0xbb);
        assert_eq!(read_dsp(&mut servo, 0x509), 0xcc);
    }
}
