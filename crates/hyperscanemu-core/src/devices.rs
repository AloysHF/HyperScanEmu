use crate::EmulatorError;

pub const CPU_CLOCK_HZ: u64 = 108_000_000;
pub const PERIPHERAL_CLOCK_HZ: u64 = 27_000_000;
pub const TIMER_INTERRUPT_SOURCE: u8 = 56;

const TIMER_BASE: u32 = 0x0816_0000;
const TIMER_BLOCK_SIZE: u32 = 0x1000;
const TIMER_COUNT: usize = 6;
const TIMER_GATE_BASE: u32 = 0x0821_006c;
const TIMER_CLOCK_SELECT: u32 = 0x0821_00e4;

#[derive(Debug, Clone)]
pub struct Spg290Devices {
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
            timers: std::array::from_fn(|_| Timer::default()),
            timer_clock_select: 0,
            pending_interrupts: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn read_u32(&self, address: u32) -> Result<u32, EmulatorError> {
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
