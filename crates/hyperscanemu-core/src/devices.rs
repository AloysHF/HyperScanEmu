use crate::{
    audio::{AudioDmaRequest, DacFifo},
    card::CardDevice,
    cdrom::{CdCommandTrace, CdDmaRequest, CdServo, CdServoState},
    uart::Uart,
    video::{DirectFrameState, PpuRenderState, VideoController},
    CardImage, EmulatorError, InputState,
};

pub const CPU_CLOCK_HZ: u64 = 108_000_000;
pub const PERIPHERAL_CLOCK_HZ: u64 = 27_000_000;
pub const TIMER_INTERRUPT_SOURCE: u8 = 56;
pub const I2C_INTERRUPT_SOURCE: u8 = 39;
pub const CD_INTERRUPT_SOURCE: u8 = 60;
pub const PPU_INTERRUPT_SOURCE: u8 = 53;
pub const SPU_INTERRUPT_SOURCE: u8 = 63;

const CD_BASE: u32 = 0x0806_0000;
const CD_END: u32 = 0x0806_ffff;
const SPU_BASE: u32 = 0x0805_0000;
const SPU_END: u32 = 0x0805_ffff;
const I2C_BASE: u32 = 0x0813_0000;
const I2C_END: u32 = 0x0813_ffff;
const C3_STUB_START: u32 = 0x0824_0000;
const C3_STUB_END: u32 = 0x0824_ffff;
const TIMER_BASE: u32 = 0x0816_0000;
const TIMER_BLOCK_SIZE: u32 = 0x1000;
const TIMER_COUNT: usize = 6;
const UART_BASE: u32 = 0x0815_0000;
const UART_END: u32 = 0x0815_0010;
const TIMER_GATE_BASE: u32 = 0x0821_006c;
const TIMER_CLOCK_SELECT: u32 = 0x0821_00e4;
const CLOCK_REGISTER_START: u32 = 0x0821_0000;
const CLOCK_REGISTER_END: u32 = 0x0821_0114;
const CLOCK_REGISTER_COUNT: usize = ((CLOCK_REGISTER_END - CLOCK_REGISTER_START) / 4 + 1) as usize;
const GPIO_OUTPUT: u32 = 0x0820_0024;
const GPIO_INPUT: u32 = 0x0820_0068;
const SYSTEM_STRAP_STATUS: u32 = 0x0820_0058;
const SYSTEM_CONFIG_START: u32 = 0x0820_0000;
const SYSTEM_CONFIG_END: u32 = 0x0820_0114;
const SYSTEM_CONFIG_REGISTER_COUNT: usize =
    ((SYSTEM_CONFIG_END - SYSTEM_CONFIG_START) / 4 + 1) as usize;
const MIU_REGISTER_START: u32 = 0x0807_000c;
const MIU_REGISTER_END: u32 = 0x0807_01fc;
const MIU_REGISTER_COUNT: usize = ((MIU_REGISTER_END - MIU_REGISTER_START) / 4 + 1) as usize;
const MIU_EXTENDED_START: u32 = 0x0823_0000;
const MIU_EXTENDED_END: u32 = 0x0823_0064;
const MIU_EXTENDED_REGISTER_COUNT: usize =
    ((MIU_EXTENDED_END - MIU_EXTENDED_START) / 4 + 1) as usize;
const BUFFER_CONTROL_START: u32 = 0x0809_0000;
const BUFFER_CONTROL_END: u32 = 0x0809_00fc;
const BUFFER_CONTROL_REGISTER_COUNT: usize =
    ((BUFFER_CONTROL_END - BUFFER_CONTROL_START) / 4 + 1) as usize;
const INTERRUPT_PENDING: u32 = 0x080a_0000;
const INTERRUPT_PENDING_HIGH: u32 = 0x080a_0004;
const INTERRUPT_PRIORITY_MASTER: u32 = 0x080a_0008;
const GPU_SOFTWARE_INTERRUPT: u32 = 0x080a_000c;
const INTERRUPT_PRIORITY_START: u32 = 0x080a_0010;
const INTERRUPT_PRIORITY_END: u32 = 0x080a_001c;
const INTERRUPT_MASK_START: u32 = 0x080a_0020;
const INTERRUPT_MASK_END: u32 = 0x080a_0024;

#[derive(Debug, Clone)]
pub struct Spg290Devices {
    audio: DacFifo,
    spu_registers: Box<[u32]>,
    cd: CdServo,
    video: VideoController,
    i2c: I2cController,
    card: CardDevice,
    uart: Uart,
    gpio_output: u32,
    system_config_registers: [u32; SYSTEM_CONFIG_REGISTER_COUNT],
    miu_registers: [u32; MIU_REGISTER_COUNT],
    miu_extended_registers: [u32; MIU_EXTENDED_REGISTER_COUNT],
    buffer_control_registers: [u32; BUFFER_CONTROL_REGISTER_COUNT],
    interrupt_priority_master: u32,
    interrupt_priorities: [u32; 4],
    interrupt_masks: [u32; 2],
    gpu_software_interrupt: bool,
    timers: [Timer; TIMER_COUNT],
    timer_clock_select: u32,
    clock_registers: [u32; CLOCK_REGISTER_COUNT],
    pending_interrupts: u64,
}

impl Default for Spg290Devices {
    fn default() -> Self {
        Self::new()
    }
}

impl Spg290Devices {
    pub fn new() -> Self {
        let mut system_config_registers = [0; SYSTEM_CONFIG_REGISTER_COUNT];
        system_config_registers[((SYSTEM_STRAP_STATUS - SYSTEM_CONFIG_START) / 4) as usize] = 3;
        Self {
            audio: DacFifo::default(),
            spu_registers: vec![0; 0x1_0000 / 4].into_boxed_slice(),
            cd: CdServo::default(),
            video: VideoController::default(),
            i2c: I2cController::default(),
            card: CardDevice::default(),
            uart: Uart::default(),
            gpio_output: 0,
            system_config_registers,
            miu_registers: [0; MIU_REGISTER_COUNT],
            miu_extended_registers: [0; MIU_EXTENDED_REGISTER_COUNT],
            buffer_control_registers: [0; BUFFER_CONTROL_REGISTER_COUNT],
            interrupt_priority_master: 0,
            interrupt_priorities: [0; 4],
            interrupt_masks: [0; 2],
            gpu_software_interrupt: false,
            timers: std::array::from_fn(|_| Timer::default()),
            timer_clock_select: 0,
            clock_registers: [0; CLOCK_REGISTER_COUNT],
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
        if let Some(value) = self.audio.read(address) {
            return Ok(value);
        }
        if (SPU_BASE..=SPU_END).contains(&address) && address & 3 == 0 {
            return Ok(self.spu_registers[((address - SPU_BASE) / 4) as usize]);
        }
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
        if (C3_STUB_START..=C3_STUB_END).contains(&address) {
            return Ok(0);
        }
        if (UART_BASE..=UART_END).contains(&address) {
            return self
                .uart
                .read(address - UART_BASE)
                .ok_or(unknown_mmio("read", address));
        }
        if address == GPIO_OUTPUT {
            return Ok(self.gpio_output);
        }
        if address == GPIO_INPUT {
            return Ok(u32::from(self.card.read_line()));
        }
        if let Some(index) = system_config_register_index(address) {
            return Ok(self.system_config_registers[index]);
        }
        if address == INTERRUPT_PENDING {
            return Ok(self.interrupt_pending_register());
        }
        if address == INTERRUPT_PENDING_HIGH {
            return Ok((self.active_interrupts() >> 24) as u32 & 0xff);
        }
        if address == INTERRUPT_PRIORITY_MASTER {
            return Ok(self.interrupt_priority_master);
        }
        if address == GPU_SOFTWARE_INTERRUPT {
            return Ok(u32::from(self.gpu_software_interrupt));
        }
        if (INTERRUPT_PRIORITY_START..=INTERRUPT_PRIORITY_END).contains(&address) {
            return Ok(
                self.interrupt_priorities[((address - INTERRUPT_PRIORITY_START) / 4) as usize]
            );
        }
        if (INTERRUPT_MASK_START..=INTERRUPT_MASK_END).contains(&address) {
            return Ok(self.interrupt_masks[((address - INTERRUPT_MASK_START) / 4) as usize]);
        }
        if let Some((timer, offset)) = timer_address(address) {
            return self.timers[timer]
                .read(offset)
                .ok_or(unknown_mmio("read", address));
        }
        if address == TIMER_CLOCK_SELECT {
            return Ok(self.timer_clock_select);
        }
        if let Some(index) = miu_register_index(address) {
            return Ok(self.miu_registers[index]);
        }
        if let Some(index) = miu_extended_register_index(address) {
            return Ok(self.miu_extended_registers[index]);
        }
        if let Some(index) = buffer_control_register_index(address) {
            return Ok(self.buffer_control_registers[index]);
        }
        if let Some(index) = clock_register_index(address) {
            return Ok(self.clock_registers[index]);
        }
        Err(unknown_mmio("read", address))
    }

    pub fn write_u32(&mut self, address: u32, value: u32) -> Result<(), EmulatorError> {
        if self.audio.write(address, value).is_some() {
            if !self.audio.interrupt_pending() {
                self.pending_interrupts &= !(1_u64 << SPU_INTERRUPT_SOURCE);
            }
            return Ok(());
        }
        if (SPU_BASE..=SPU_END).contains(&address) && address & 3 == 0 {
            self.spu_registers[((address - SPU_BASE) / 4) as usize] = value;
            return Ok(());
        }
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
        if (C3_STUB_START..=C3_STUB_END).contains(&address) {
            return Ok(());
        }
        if (UART_BASE..=UART_END).contains(&address) {
            return self
                .uart
                .write(address - UART_BASE, value)
                .ok_or(unknown_mmio("write", address));
        }
        if address == GPIO_OUTPUT {
            self.gpio_output = value;
            self.card.write_line(value & 2 != 0);
            return Ok(());
        }
        if let Some(index) = system_config_register_index(address) {
            self.system_config_registers[index] = value;
            return Ok(());
        }
        if address == INTERRUPT_PRIORITY_MASTER {
            self.interrupt_priority_master = value & 0xff;
            return Ok(());
        }
        if address == GPU_SOFTWARE_INTERRUPT {
            if value & (1 << 8) != 0 {
                self.gpu_software_interrupt = false;
                if !self.video.interrupt_pending() {
                    self.pending_interrupts &= !(1_u64 << PPU_INTERRUPT_SOURCE);
                }
            }
            if value & 1 != 0 {
                self.gpu_software_interrupt = true;
                self.pending_interrupts |= 1_u64 << PPU_INTERRUPT_SOURCE;
            }
            return Ok(());
        }
        if (INTERRUPT_PRIORITY_START..=INTERRUPT_PRIORITY_END).contains(&address) {
            self.interrupt_priorities[((address - INTERRUPT_PRIORITY_START) / 4) as usize] =
                value & 0x00ff_ffff;
            return Ok(());
        }
        if (INTERRUPT_MASK_START..=INTERRUPT_MASK_END).contains(&address) {
            self.interrupt_masks[((address - INTERRUPT_MASK_START) / 4) as usize] = value;
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
        if let Some(index) = miu_register_index(address) {
            self.miu_registers[index] = value;
            return Ok(());
        }
        if let Some(index) = miu_extended_register_index(address) {
            self.miu_extended_registers[index] = value;
            return Ok(());
        }
        if let Some(index) = buffer_control_register_index(address) {
            self.buffer_control_registers[index] = value;
            return Ok(());
        }
        if let Some(index) = clock_register_index(address) {
            self.clock_registers[index] = value;
            return Ok(());
        }
        Err(unknown_mmio("write", address))
    }

    pub fn tick(&mut self, cpu_cycles: u64) -> Result<(), EmulatorError> {
        self.card.tick(cpu_cycles);
        if self.audio.tick(cpu_cycles) {
            self.pending_interrupts |= 1_u64 << SPU_INTERRUPT_SOURCE;
        }
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

    pub(crate) fn cd_command_trace(&self) -> Vec<CdCommandTrace> {
        self.cd.command_trace()
    }

    pub(crate) fn set_execution_context(&mut self, pc: u32, link: u32) {
        self.cd.set_execution_context(pc, link);
    }

    pub(crate) fn cd_servo_state(&self) -> CdServoState {
        self.cd.state()
    }

    pub(crate) fn external_rom_selected(&self) -> bool {
        let index = ((0x0820_0004 - SYSTEM_CONFIG_START) / 4) as usize;
        self.system_config_registers[index] & (1 << 24) != 0
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

    pub(crate) fn ppu_render_state(&self) -> PpuRenderState {
        self.video.ppu_render_state()
    }

    pub fn cycles_until_frame_end(&self) -> u64 {
        self.video.cycles_until_frame_end()
    }

    pub(crate) fn take_audio_dma_request(&mut self) -> Option<AudioDmaRequest> {
        self.audio.take_dma_request()
    }

    pub(crate) fn complete_audio_dma(&mut self, left: u16, right: u16) {
        self.audio.complete_dma(left, right);
    }

    pub(crate) fn drain_audio_samples(&mut self, destination: &mut Vec<i16>) {
        self.audio.drain_samples(destination);
    }

    pub(crate) fn drain_uart_output(&mut self, destination: &mut Vec<u8>) {
        self.uart.drain_output(destination);
    }

    fn cd_disc_sectors(&self) -> Option<u32> {
        self.cd.disc_sector_count()
    }

    fn active_interrupts(&self) -> u64 {
        let mut sources = 0;
        if self.i2c.interrupt_pending() {
            sources |= 1_u64 << I2C_INTERRUPT_SOURCE;
        }
        if self.timers.iter().any(Timer::interrupt_pending) {
            sources |= 1_u64 << TIMER_INTERRUPT_SOURCE;
        }
        if self.cd.frame_found() {
            sources |= 1_u64 << CD_INTERRUPT_SOURCE;
        }
        if self.video.interrupt_pending() || self.gpu_software_interrupt {
            sources |= 1_u64 << PPU_INTERRUPT_SOURCE;
        }
        if self.audio.interrupt_pending() {
            sources |= 1_u64 << SPU_INTERRUPT_SOURCE;
        }
        sources
    }

    fn interrupt_pending_register(&self) -> u32 {
        let sources = self.active_interrupts();
        (32..64).fold(0_u32, |pending, vector| {
            if sources & (1_u64 << vector) != 0 {
                pending | (1 << (63 - vector))
            } else {
                pending
            }
        })
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

fn clock_register_index(address: u32) -> Option<usize> {
    if !(CLOCK_REGISTER_START..=CLOCK_REGISTER_END).contains(&address) || address & 3 != 0 {
        return None;
    }
    Some(((address - CLOCK_REGISTER_START) / 4) as usize)
}

fn system_config_register_index(address: u32) -> Option<usize> {
    if !(SYSTEM_CONFIG_START..=SYSTEM_CONFIG_END).contains(&address) || address & 3 != 0 {
        return None;
    }
    Some(((address - SYSTEM_CONFIG_START) / 4) as usize)
}

fn miu_register_index(address: u32) -> Option<usize> {
    if !(MIU_REGISTER_START..=MIU_REGISTER_END).contains(&address) || address & 3 != 0 {
        return None;
    }
    Some(((address - MIU_REGISTER_START) / 4) as usize)
}

fn miu_extended_register_index(address: u32) -> Option<usize> {
    if !(MIU_EXTENDED_START..=MIU_EXTENDED_END).contains(&address) || address & 3 != 0 {
        return None;
    }
    Some(((address - MIU_EXTENDED_START) / 4) as usize)
}

fn buffer_control_register_index(address: u32) -> Option<usize> {
    if !(BUFFER_CONTROL_START..=BUFFER_CONTROL_END).contains(&address) || address & 3 != 0 {
        return None;
    }
    Some(((address - BUFFER_CONTROL_START) / 4) as usize)
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
    fn interrupt_controller_reports_vector_mapped_pending_bits() {
        let mut devices = Spg290Devices::new();
        devices.write_u32(I2C_BASE + 0x24, 2).unwrap();
        devices.write_u32(I2C_BASE + 0x20, 1).unwrap();
        devices.tick(608).unwrap();

        assert_eq!(
            devices.read_u32(INTERRUPT_PENDING).unwrap(),
            1 << (63 - I2C_INTERRUPT_SOURCE)
        );
        devices.write_u32(GPU_SOFTWARE_INTERRUPT, 1).unwrap();
        assert_ne!(
            devices.read_u32(INTERRUPT_PENDING).unwrap() & (1 << (63 - PPU_INTERRUPT_SOURCE)),
            0
        );
        devices.write_u32(GPU_SOFTWARE_INTERRUPT, 1 << 8).unwrap();
        assert_eq!(devices.read_u32(GPU_SOFTWARE_INTERRUPT).unwrap(), 0);
    }

    #[test]
    fn interrupt_priority_configuration_round_trips() {
        let mut devices = Spg290Devices::new();
        devices.write_u32(INTERRUPT_PRIORITY_MASTER, 0x1ff).unwrap();
        devices
            .write_u32(INTERRUPT_PRIORITY_START + 8, 0xffff_ffff)
            .unwrap();

        assert_eq!(devices.read_u32(INTERRUPT_PRIORITY_MASTER).unwrap(), 0xff);
        assert_eq!(
            devices.read_u32(INTERRUPT_PRIORITY_START + 8).unwrap(),
            0x00ff_ffff
        );
    }

    #[test]
    fn clock_configuration_registers_round_trip() {
        let mut devices = Spg290Devices::new();

        devices.write_u32(0x0821_005c, 0x102).unwrap();

        assert_eq!(devices.read_u32(0x0821_005c).unwrap(), 0x102);
    }

    #[test]
    fn system_strap_reports_external_rom_boot_configuration() {
        let devices = Spg290Devices::new();

        assert_eq!(devices.read_u32(SYSTEM_STRAP_STATUS).unwrap(), 3);
    }

    #[test]
    fn miu_configuration_registers_round_trip() {
        let mut devices = Spg290Devices::new();

        devices.write_u32(0x0807_0060, 0x8000_1234).unwrap();

        assert_eq!(devices.read_u32(0x0807_0060).unwrap(), 0x8000_1234);
    }

    #[test]
    fn spu_register_and_internal_sram_windows_round_trip() {
        let mut devices = Spg290Devices::new();

        devices.write_u32(SPU_BASE, 0x00ff).unwrap();
        devices.write_u32(SPU_BASE + 0xc000, 0x1234_5678).unwrap();

        assert_eq!(devices.read_u32(SPU_BASE).unwrap(), 0x00ff);
        assert_eq!(devices.read_u32(SPU_BASE + 0xc000).unwrap(), 0x1234_5678);
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

    #[test]
    fn c3_ecc_window_accepts_firmware_probe_writes() {
        let mut devices = Spg290Devices::new();

        devices.write_u32(C3_STUB_START + 0x10, 1).unwrap();

        assert_eq!(devices.read_u32(C3_STUB_START + 0x10), Ok(0));
    }

    #[test]
    fn extended_miu_registers_retain_configuration() {
        let mut devices = Spg290Devices::new();

        devices
            .write_u32(MIU_EXTENDED_START + 0x44, 0x1234_5678)
            .unwrap();
        devices
            .write_u32(MIU_EXTENDED_START + 0x64, 0x9abc_def0)
            .unwrap();

        assert_eq!(devices.read_u32(MIU_EXTENDED_START + 0x44), Ok(0x1234_5678));
        assert_eq!(devices.read_u32(MIU_EXTENDED_START + 0x64), Ok(0x9abc_def0));
        assert!(matches!(
            devices.write_u32(MIU_EXTENDED_END + 4, 1),
            Err(EmulatorError::UnknownMmio { .. })
        ));
    }
}
