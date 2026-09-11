use crate::{
    card::CardDevice,
    cdrom::{CdDmaRequest, CdServo},
    video::{DirectFrameState, VideoController},
    CardImage, EmulatorError, InputState,
};

pub const CPU_CLOCK_HZ: u64 = 108_000_000;
pub const PERIPHERAL_CLOCK_HZ: u64 = 27_000_000;
pub const TIMER_INTERRUPT_SOURCE: u8 = 56;
pub const I2C_INTERRUPT_SOURCE: u8 = 39;
pub const CD_INTERRUPT_SOURCE: u8 = 60;
pub const PPU_INTERRUPT_SOURCE: u8 = 53;

const CD_BASE: u32 = 0x0806_0000;
const CD_END: u32 = 0x0806_ffff;
const I2C_BASE: u32 = 0x0813_0000;
const I2C_END: u32 = 0x0813_ffff;
const TIMER_BASE: u32 = 0x0816_0000;
const TIMER_BLOCK_SIZE: u32 = 0x1000;
const TIMER_COUNT: usize = 6;
const TIMER_GATE_BASE: u32 = 0x0821_006c;
const TIMER_CLOCK_SELECT: u32 = 0x0821_00e4;
const GPIO_OUTPUT: u32 = 0x0820_0024;
const GPIO_INPUT: u32 = 0x0820_0068;

#[derive(Debug, Clone)]
pub struct Spg290Devices {
    cd: CdServo,
    video: VideoController,
    i2c: I2cController,
    card: CardDevice,
    gpio_output: u32,
    timers: [Timer; TIMER_COUNT],
    timer_clock_select: u32,
    pending_interrupts: u64,
}

impl Default for Spg290Devices {
    fn default() -> Self {
        Self::new()
    }
}

impl Spg290Devices {
    pub fn new() -> Self {
        Self {
            cd: CdServo::default(),
            video: VideoController::default(),
            i2c: I2cController::default(),
            card: CardDevice::default(),
            gpio_output: 0,
            timers: std::array::from_fn(|_| Timer::default()),
            timer_clock_select: 0,
            pending_interrupts: 0,
        }
    }

    pub fn reset(&mut self) {
        let card = self.card.eject();
        let disc_sectors = self.cd_disc_sectors();
        *self = Self::new();
        self.cd.set_disc(disc_sectors);
        if let Some(card) = card {
            self.card.insert(card);
        }
    }

    pub fn read_u32(&self, address: u32) -> Result<u32, EmulatorError> {
        if let Some(value) = self.video.read(address) {
            return Ok(value);
        }
        if (CD_BASE..=CD_END).contains(&address) {
            return self
                .cd
                .read(address - CD_BASE)
                .ok_or(unknown_mmio("read", address));
        }
        if (I2C_BASE..=I2C_END).contains(&address) {
            return self.i2c.read(address - I2C_BASE);
        }
        if address == GPIO_OUTPUT {
            return Ok(self.gpio_output);
        }
        if address == GPIO_INPUT {
            return Ok(u32::from(self.card.read_line()));
        }
        if let Some((timer, offset)) = timer_address(address) {
            return self.timers[timer]
                .read(offset)
                .ok_or(unknown_mmio("read", address));
        }
        if address == TIMER_CLOCK_SELECT {
            return Ok(self.timer_clock_select);
        }
        Err(unknown_mmio("read", address))
    }

    pub fn write_u32(&mut self, address: u32, value: u32) -> Result<(), EmulatorError> {
        if self.video.write(address, value).is_some() {
            if !self.video.interrupt_pending() {
                self.pending_interrupts &= !(1_u64 << PPU_INTERRUPT_SOURCE);
            }
            return Ok(());
        }
        if (CD_BASE..=CD_END).contains(&address) {
            self.cd
                .write(address - CD_BASE, value)
                .ok_or(unknown_mmio("write", address))?;
            if !self.cd.frame_found() {
                self.pending_interrupts &= !(1_u64 << CD_INTERRUPT_SOURCE);
            }
            return Ok(());
        }
        if (I2C_BASE..=I2C_END).contains(&address) {
            self.i2c.write(address - I2C_BASE, value)?;
            if !self.i2c.interrupt_pending() {
                self.pending_interrupts &= !(1_u64 << I2C_INTERRUPT_SOURCE);
            }
            return Ok(());
        }
        if address == GPIO_OUTPUT {
            self.gpio_output = value;
            self.card.write_line(value & 2 != 0);
            return Ok(());
        }
        if let Some((timer, offset)) = timer_address(address) {
            self.timers[timer]
                .write(offset, value)
                .ok_or(unknown_mmio("write", address))?;
            if !self.timers[timer].interrupt_pending() {
                self.pending_interrupts &= !(1_u64 << TIMER_INTERRUPT_SOURCE);
            }
            return Ok(());
        }
        if address == TIMER_CLOCK_SELECT {
            self.timer_clock_select = value;
            self.update_timer_clocks();
            return Ok(());
        }
        if let Some(timer) = timer_gate_index(address) {
            self.timers[timer].set_gate(value);
            return Ok(());
        }
        Err(unknown_mmio("write", address))
    }

    pub fn tick(&mut self, cpu_cycles: u64) -> Result<(), EmulatorError> {
        self.card.tick(cpu_cycles);
        self.cd.tick(cpu_cycles)?;
        if self.video.tick(cpu_cycles) {
            self.pending_interrupts |= 1_u64 << PPU_INTERRUPT_SOURCE;
        }
        if self.i2c.tick(cpu_cycles) {
            self.pending_interrupts |= 1_u64 << I2C_INTERRUPT_SOURCE;
        }
        for timer in &mut self.timers {
            if timer.tick(cpu_cycles)? {
                self.pending_interrupts |= 1_u64 << TIMER_INTERRUPT_SOURCE;
            }
        }
        Ok(())
    }

    pub fn take_pending_interrupts(&mut self) -> u64 {
        std::mem::take(&mut self.pending_interrupts)
    }

    pub fn set_input(&mut self, input: InputState) {
        self.i2c.set_input(input);
    }

    pub fn insert_card(&mut self, card: CardImage) -> Option<CardImage> {
        self.card.insert(card)
    }

    pub fn eject_card(&mut self) -> Option<CardImage> {
        self.card.eject()
    }

    pub fn set_disc(&mut self, sector_count: Option<u32>) {
        self.cd.set_disc(sector_count);
    }

    pub(crate) fn take_cd_dma_request(&mut self) -> Option<CdDmaRequest> {
        self.cd.take_dma_request()
    }

    pub(crate) fn complete_cd_dma(&mut self, next_pointer: u32) {
        self.cd.complete_dma(next_pointer);
        self.pending_interrupts |= 1_u64 << CD_INTERRUPT_SOURCE;
    }

    pub(crate) fn direct_frame_state(&self) -> DirectFrameState {
        self.video.direct_frame_state()
    }

    pub fn cycles_until_frame_end(&self) -> u64 {
        self.video.cycles_until_frame_end()
    }

    fn cd_disc_sectors(&self) -> Option<u32> {
        self.cd.disc_sector_count()
    }

    fn update_timer_clocks(&mut self) {
        let divider = u64::from((self.timer_clock_select & 0xff) + 1);
        for (index, timer) in self.timers.iter_mut().enumerate() {
            let low_frequency = self.timer_clock_select & (1 << (8 + index)) != 0;
            timer.clock_hz = if low_frequency {
                32_768
            } else {
                PERIPHERAL_CLOCK_HZ / divider
            };
        }
    }
}

#[derive(Debug, Clone, Default)]
struct I2cController {
    config: u32,
    irq_control: u32,
    clock_conf: u32,
    id: u32,
    port_addr: u32,
    write_data: u32,
    read_data: u32,
    remaining_cycles: Option<u64>,
    repeat: bool,
    input: InputState,
    sampled: [[u8; 4]; 2],
}

impl I2cController {
    fn read(&self, offset: u32) -> Result<u32, EmulatorError> {
        match offset {
            0x20 => Ok(self.config),
            0x24 => Ok(self.irq_control),
            0x28 => Ok(self.clock_conf),
            0x2c => Ok(self.id),
            0x30 => Ok(self.port_addr),
            0x34 => Ok(self.write_data),
            0x38 => Ok(self.read_data),
            _ => Err(unknown_mmio("read", I2C_BASE + offset)),
        }
    }

    fn write(&mut self, offset: u32, value: u32) -> Result<(), EmulatorError> {
        match offset {
            0x20 => {
                self.config = value;
                self.schedule_transfer();
            }
            0x24 => {
                self.irq_control = value;
                if value & 1 != 0 {
                    self.irq_control &= !1;
                }
            }
            0x28 => self.clock_conf = value,
            0x2c => self.id = value,
            0x30 => self.port_addr = value,
            0x34 => self.write_data = value,
            0x38 => self.read_data = value,
            _ => return Err(unknown_mmio("write", I2C_BASE + offset)),
        }
        Ok(())
    }

    fn schedule_transfer(&mut self) {
        let clocks = if self.config & 1 != 0 {
            self.repeat = false;
            Some(38)
        } else if self.config & 2 != 0 {
            self.repeat = false;
            Some(47)
        } else if self.config & 4 != 0 {
            self.repeat = true;
            Some(38)
        } else {
            self.repeat = false;
            None
        };

        self.remaining_cycles = clocks.map(|clocks| self.cycles_for_clocks(clocks));
        if self.config & 0x100 != 0 && clocks.is_none() {
            self.remaining_cycles = None;
        }
    }

    fn cycles_for_clocks(&self, clocks: u64) -> u64 {
        let divider = u64::from((self.clock_conf & 0x3ff) + 1);
        clocks * 16 * divider
    }

    fn tick(&mut self, mut cpu_cycles: u64) -> bool {
        let mut raised = false;
        while let Some(remaining) = self.remaining_cycles {
            if cpu_cycles < remaining {
                self.remaining_cycles = Some(remaining - cpu_cycles);
                break;
            }

            cpu_cycles -= remaining;
            raised |= self.complete_transfer();
            self.remaining_cycles = if self.repeat {
                Some(self.cycles_for_clocks(38))
            } else {
                None
            };
        }
        raised
    }

    fn complete_transfer(&mut self) -> bool {
        if self.config & 0x40 != 0 {
            self.read_data = u32::from(self.read_controller());
        }
        self.config |= (self.config & 7) << 3;
        if self.irq_control & 2 != 0 {
            self.irq_control |= 1;
            true
        } else {
            false
        }
    }

    fn read_controller(&mut self) -> u16 {
        let port = ((self.port_addr >> 4) & 0x0f) as usize;
        let address = (self.port_addr & 0x0f) as usize;
        if port >= self.input.controllers.len() {
            return 0;
        }

        if address < 4 {
            let value = self.input.controllers[port].packet()[address];
            self.sampled[port][address] = value;
            u16::from(value)
        } else {
            let sum = self.sampled[port]
                .iter()
                .fold(0_u16, |sum, value| sum.wrapping_add(u16::from(*value)));
            (sum << 2) | port as u16
        }
    }

    fn set_input(&mut self, input: InputState) {
        self.input = input;
    }

    fn interrupt_pending(&self) -> bool {
        self.irq_control & 1 != 0
    }
}

#[derive(Debug, Clone)]
struct Timer {
    gate_enabled: bool,
    control: u32,
    control2: u32,
    preload: u32,
    counter: u16,
    compare_capture: u32,
    upcount: u32,
    clock_hz: u64,
    phase: u64,
}

impl Default for Timer {
    fn default() -> Self {
        Self {
            gate_enabled: false,
            control: 0,
            control2: 0,
            preload: 0,
            counter: 0,
            compare_capture: 0,
            upcount: 0,
            clock_hz: PERIPHERAL_CLOCK_HZ,
            phase: 0,
        }
    }
}

impl Timer {
    fn read(&self, offset: u32) -> Option<u32> {
        match offset {
            0x00 => Some((self.control & !0xffff) | u32::from(self.counter)),
            0x04 => Some(self.control2),
            0x08 => Some(self.preload),
            0x0c => Some(self.compare_capture),
            0x10 => Some(self.upcount),
            _ => None,
        }
    }

    fn write(&mut self, offset: u32, value: u32) -> Option<()> {
        match offset {
            0x00 => {
                let acknowledge = value & (1 << 26) != 0;
                self.control = (value & !(1 << 26)) | (self.control & (1 << 26));
                if acknowledge {
                    self.control &= !(1 << 26);
                }
            }
            0x04 => self.control2 = value,
            0x08 => self.preload = value,
            0x0c => self.compare_capture = value,
            0x10 => self.upcount = value,
            _ => return None,
        }
        Some(())
    }

    fn set_gate(&mut self, value: u32) {
        self.gate_enabled = value & 1 != 0;
        if value & 2 != 0 {
            self.counter = self.preload as u16;
            self.phase = 0;
        }
    }

    fn tick(&mut self, cpu_cycles: u64) -> Result<bool, EmulatorError> {
        if !self.gate_enabled || self.control & (1 << 31) == 0 {
            return Ok(false);
        }
        let mode = (self.control2 >> 30) & 3;
        if mode != 0 {
            return Err(EmulatorError::UnsupportedTimerMode { mode: mode as u8 });
        }

        self.phase = self
            .phase
            .wrapping_add(cpu_cycles.saturating_mul(self.clock_hz));
        let ticks = self.phase / CPU_CLOCK_HZ;
        self.phase %= CPU_CLOCK_HZ;
        let mut raised = false;
        for _ in 0..ticks {
            if self.counter == u16::MAX {
                self.counter = self.preload as u16;
                if self.control & (1 << 27) != 0 {
                    self.control |= 1 << 26;
                    raised = true;
                }
            } else {
                self.counter += 1;
            }
        }
        Ok(raised)
    }

    fn interrupt_pending(&self) -> bool {
        self.control & (1 << 26) != 0
    }
}

fn timer_address(address: u32) -> Option<(usize, u32)> {
    if !(TIMER_BASE..TIMER_BASE + TIMER_BLOCK_SIZE * TIMER_COUNT as u32).contains(&address) {
        return None;
    }
    let relative = address - TIMER_BASE;
    Some((
        (relative / TIMER_BLOCK_SIZE) as usize,
        relative % TIMER_BLOCK_SIZE,
    ))
}

fn timer_gate_index(address: u32) -> Option<usize> {
    let relative = address.checked_sub(TIMER_GATE_BASE)?;
    if relative.is_multiple_of(4) && relative / 4 < TIMER_COUNT as u32 {
        Some((relative / 4) as usize)
    } else {
        None
    }
}

fn unknown_mmio(access: &'static str, address: u32) -> EmulatorError {
    EmulatorError::UnknownMmio { access, address }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ControllerButton, ControllerState};

    fn finish_i2c_read(devices: &mut Spg290Devices, port_address: u32) -> u32 {
        devices.write_u32(I2C_BASE + 0x30, port_address).unwrap();
        devices.write_u32(I2C_BASE + 0x20, 0x41).unwrap();
        devices.tick(608).unwrap();
        devices.read_u32(I2C_BASE + 0x38).unwrap()
    }

    #[test]
    fn i2c_reads_both_controller_packets_and_checksums() {
        let mut first = ControllerState::default();
        first.set_button(ControllerButton::Start, true);
        first.set_button(ControllerButton::Red, true);
        first.analog_x = 0x11;
        first.analog_y = 0x22;
        let mut second = ControllerState::default();
        second.set_button(ControllerButton::Green, true);
        second.analog_x = 0x33;
        second.analog_y = 0x44;

        let mut devices = Spg290Devices::new();
        devices.set_input(InputState {
            controllers: [first, second],
        });

        assert_eq!(finish_i2c_read(&mut devices, 0x00), 0x02);
        assert_eq!(finish_i2c_read(&mut devices, 0x01), 0x40);
        assert_eq!(finish_i2c_read(&mut devices, 0x02), 0x22);
        assert_eq!(finish_i2c_read(&mut devices, 0x03), 0x11);
        assert_eq!(finish_i2c_read(&mut devices, 0x04), 0x1d4);

        assert_eq!(finish_i2c_read(&mut devices, 0x10), 0x00);
        assert_eq!(finish_i2c_read(&mut devices, 0x11), 0x80);
        assert_eq!(finish_i2c_read(&mut devices, 0x12), 0x44);
        assert_eq!(finish_i2c_read(&mut devices, 0x13), 0x33);
        assert_eq!(finish_i2c_read(&mut devices, 0x14), 0x3dd);
    }

    #[test]
    fn i2c_completion_obeys_clock_and_sets_ack() {
        let mut devices = Spg290Devices::new();
        devices.write_u32(I2C_BASE + 0x30, 2).unwrap();
        devices.write_u32(I2C_BASE + 0x20, 0x41).unwrap();

        devices.tick(607).unwrap();
        assert_eq!(devices.read_u32(I2C_BASE + 0x20).unwrap() & 8, 0);
        devices.tick(1).unwrap();

        assert_ne!(devices.read_u32(I2C_BASE + 0x20).unwrap() & 8, 0);
        assert_eq!(devices.read_u32(I2C_BASE + 0x38).unwrap(), 0x7f);
    }

    #[test]
    fn i2c_interrupt_can_be_acknowledged() {
        let mut devices = Spg290Devices::new();
        devices.write_u32(I2C_BASE + 0x24, 2).unwrap();
        devices.write_u32(I2C_BASE + 0x20, 1).unwrap();
        devices.tick(608).unwrap();

        assert_eq!(
            devices.take_pending_interrupts(),
            1_u64 << I2C_INTERRUPT_SOURCE
        );
        assert_eq!(devices.read_u32(I2C_BASE + 0x24).unwrap(), 3);

        devices.write_u32(I2C_BASE + 0x24, 3).unwrap();
        assert_eq!(devices.read_u32(I2C_BASE + 0x24).unwrap(), 2);
        assert_eq!(devices.take_pending_interrupts(), 0);
    }

    #[test]
    fn timer_overflow_reloads_and_raises_shared_interrupt() {
        let mut devices = Spg290Devices::new();
        devices.write_u32(TIMER_BASE + 0x08, 0xfffe).unwrap();
        devices
            .write_u32(TIMER_BASE, (1 << 31) | (1 << 27))
            .unwrap();
        devices.write_u32(TIMER_GATE_BASE, 3).unwrap();

        devices.tick(8).unwrap();

        assert_eq!(devices.read_u32(TIMER_BASE).unwrap() & 0xffff, 0xfffe);
        assert_ne!(devices.read_u32(TIMER_BASE).unwrap() & (1 << 26), 0);
        assert_eq!(
            devices.take_pending_interrupts(),
            1_u64 << TIMER_INTERRUPT_SOURCE
        );
    }

    #[test]
    fn timer_acknowledge_clears_pending_status() {
        let mut devices = Spg290Devices::new();
        devices.write_u32(TIMER_BASE + 0x08, 0xffff).unwrap();
        devices
            .write_u32(TIMER_BASE, (1 << 31) | (1 << 27))
            .unwrap();
        devices.write_u32(TIMER_GATE_BASE, 3).unwrap();
        devices.tick(4).unwrap();

        devices.write_u32(TIMER_BASE, 1 << 26).unwrap();

        assert_eq!(devices.read_u32(TIMER_BASE).unwrap() & (1 << 26), 0);
    }

    #[test]
    fn unsupported_mode_is_observable() {
        let mut devices = Spg290Devices::new();
        devices.write_u32(TIMER_BASE + 0x04, 1 << 30).unwrap();
        devices.write_u32(TIMER_BASE, 1 << 31).unwrap();
        devices.write_u32(TIMER_GATE_BASE, 1).unwrap();

        assert_eq!(
            devices.tick(4),
            Err(EmulatorError::UnsupportedTimerMode { mode: 1 })
        );
    }
}
