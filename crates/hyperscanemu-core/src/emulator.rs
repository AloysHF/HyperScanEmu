use crate::{
    Bus, CardImage, DiscImage, EmulatorError, Firmware, InputState, Score7, StepOutcome,
    CYCLES_PER_INSTRUCTION_ESTIMATE,
};

pub const DISPLAY_WIDTH: usize = 640;
pub const DISPLAY_HEIGHT: usize = 480;
pub const EMULATION_STRATEGY: &str = "lle";
const INITIAL_DISPLAY_WIDTH: usize = 320;
const INITIAL_DISPLAY_HEIGHT: usize = 240;

/// How the machine enters the game: through the real firmware (LLE) or
/// through the HLE boot path without firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BootMode {
    /// Boot through the real firmware ROMs.
    #[default]
    Firmware,
    /// Boot the game directly with the measured handoff state.
    Hle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionReport {
    pub instructions: u64,
    pub cycles: u64,
    pub start_pc: u32,
    pub end_pc: u32,
    pub exceptions: u64,
}

#[derive(Debug)]
pub struct Emulator {
    cpu: Score7,
    bus: Bus,
    disc: Option<DiscImage>,
    boot_mode: BootMode,
    framebuffer: Vec<u32>,
    audio_samples: Vec<i16>,
    uart_output: Vec<u8>,
    input: InputState,
    frame_index: u64,
    display_width: usize,
    display_height: usize,
}
impl Emulator {
    pub fn new(firmware: Firmware) -> Self {
        Self::with_bus(Bus::new(firmware), BootMode::Firmware)
    }

    /// Machine without firmware; call [`Emulator::boot_hle`] after attaching
    /// a disc.
    pub fn new_hle() -> Self {
        Self::with_bus(Bus::new_hle(), BootMode::Hle)
    }

    fn with_bus(bus: Bus, boot_mode: BootMode) -> Self {
        Self {
            cpu: Score7::new(),
            bus,
            disc: None,
            boot_mode,
            framebuffer: vec![0; INITIAL_DISPLAY_WIDTH * INITIAL_DISPLAY_HEIGHT],
            audio_samples: Vec::new(),
            uart_output: Vec::new(),
            input: InputState::default(),
            frame_index: 0,
            display_width: INITIAL_DISPLAY_WIDTH,
            display_height: INITIAL_DISPLAY_HEIGHT,
        }
    }

    /// Boot mode selected for this machine.
    pub fn boot_mode(&self) -> BootMode {
        self.boot_mode
    }

    /// Emulation strategy in effect: `lle` with firmware, `hle` without.
    pub fn emulation_strategy(&self) -> &'static str {
        match self.boot_mode {
            BootMode::Firmware => EMULATION_STRATEGY,
            BootMode::Hle => "hle",
        }
    }

    pub fn reset(&mut self) -> Result<(), EmulatorError> {
        self.cpu.reset();
        self.bus.reset();
        self.bus.set_disc(self.disc.as_ref());
        self.framebuffer
            .resize(INITIAL_DISPLAY_WIDTH * INITIAL_DISPLAY_HEIGHT, 0);
        self.framebuffer.fill(0);
        self.audio_samples.clear();
        self.uart_output.clear();
        self.input = InputState::default();
        self.frame_index = 0;
        self.display_width = INITIAL_DISPLAY_WIDTH;
        self.display_height = INITIAL_DISPLAY_HEIGHT;
        if self.boot_mode == BootMode::Hle {
            let disc = self.disc.as_ref().ok_or(EmulatorError::InvalidDisc(
                "HLE boot requires an attached game disc",
            ))?;
            crate::hle_boot::boot(&mut self.cpu, &mut self.bus, disc)?;
        }
        Ok(())
    }

    /// Boot the attached disc through the HLE path without firmware.
    pub fn boot_hle(&mut self) -> Result<(), EmulatorError> {
        self.boot_mode = BootMode::Hle;
        self.reset()
    }

    pub fn set_input(&mut self, input: InputState) {
        self.input = input;
        self.bus.set_input(input);
    }

    pub fn attach_disc(&mut self, disc: DiscImage) {
        self.disc = Some(disc);
        self.bus.set_disc(self.disc.as_ref());
    }

    pub fn eject_disc(&mut self) -> Option<DiscImage> {
        let disc = self.disc.take();
        self.bus.set_disc(None);
        disc
    }

    pub fn insert_card(&mut self, card: CardImage) -> Option<CardImage> {
        self.bus.insert_card(card)
    }

    pub fn eject_card(&mut self) -> Option<CardImage> {
        self.bus.eject_card()
    }

    pub fn disc(&self) -> Option<&DiscImage> {
        self.disc.as_ref()
    }

    pub fn cd_command_trace(&self) -> Vec<crate::CdCommandTrace> {
        self.bus.cd_command_trace()
    }

    pub fn cd_servo_state(&self) -> crate::CdServoState {
        self.bus.cd_servo_state()
    }

    /// End-of-run firmware activity summary, `None` while the
    /// `HYPERSCANEMU_TRACE_BIOS_FETCH` trace is disabled.
    pub fn bios_activity_summary(&self) -> Option<String> {
        self.bus.bios_activity_summary()
    }

    /// Take the DRAM image captured at the BIOS-to-game handoff, if the
    /// trace captured one.
    pub fn take_handoff_dram(&mut self) -> Option<Vec<u8>> {
        self.bus.take_handoff_dram()
    }

    pub fn cpu(&self) -> &Score7 {
        &self.cpu
    }

    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    pub fn step(&mut self) -> Result<StepOutcome, EmulatorError> {
        self.bus
            .set_execution_context(self.cpu.pc(), self.cpu.register(3).unwrap_or(0));
        let outcome = self.cpu.step(&mut self.bus)?;
        if !matches!(outcome, StepOutcome::Exception { width: 0, .. }) {
            self.bus.tick(CYCLES_PER_INSTRUCTION_ESTIMATE)?;
        }
        if let Some(disc) = &self.disc {
            self.bus.service_cd(disc)?;
        }
        let pending = self.bus.take_pending_interrupts();
        for source in 1..64 {
            if pending & (1_u64 << source) != 0 {
                self.cpu.request_interrupt(source)?;
            }
        }
        Ok(outcome)
    }

    pub fn run_instructions(&mut self, budget: u64) -> Result<ExecutionReport, EmulatorError> {
        let start_pc = self.cpu.pc();
        let start_cycles = self.cpu.cycles();
        let mut exceptions = 0;
        for _ in 0..budget {
            if matches!(self.step()?, StepOutcome::Exception { .. }) {
                exceptions += 1;
            }
        }
        Ok(ExecutionReport {
            instructions: budget,
            cycles: self.cpu.cycles().wrapping_sub(start_cycles),
            start_pc,
            end_pc: self.cpu.pc(),
            exceptions,
        })
    }

    pub fn run_frame(&mut self) -> Result<(), EmulatorError> {
        let cycles = self.bus.cycles_until_frame_end();
        let instructions = cycles.div_ceil(CYCLES_PER_INSTRUCTION_ESTIMATE);
        self.run_instructions(instructions)?;
        let (width, height) = self.bus.render_frame(&mut self.framebuffer)?;
        self.bus.drain_audio_samples(&mut self.audio_samples);
        self.display_width = width;
        self.display_height = height;
        self.frame_index = self.frame_index.wrapping_add(1);
        Ok(())
    }

    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    pub fn audio_samples(&self) -> &[i16] {
        &self.audio_samples
    }

    pub fn drain_uart_output(&mut self) -> Vec<u8> {
        self.bus.drain_uart_output(&mut self.uart_output);
        std::mem::take(&mut self.uart_output)
    }

    pub fn frame_index(&self) -> u64 {
        self.frame_index
    }

    pub fn display_size(&self) -> (usize, usize) {
        (self.display_width, self.display_height)
    }

    pub fn firmware_fingerprint(&self) -> u64 {
        self.bus.firmware_fingerprint()
    }

    /// Size of the payload written by [`Emulator::serialize`], in bytes. The
    /// layout is fixed, so the value is stable while the content is loaded and
    /// can be reported to the frontend once (libretro reads it before saving).
    pub fn state_size(&self) -> usize {
        Self::STATE_SIZE
    }

    pub const STATE_SIZE: usize = 8
        + 8
        + 4
        + 4
        + 8
        + DISPLAY_WIDTH * DISPLAY_HEIGHT * 4
        + InputState::STATE_SIZE
        + Score7::STATE_SIZE
        + Bus::STATE_SIZE;

    /// Serialize the complete machine state (CPU, memory, peripherals, video
    /// output). Transient output buffers such as the audio queue are rebuilt by
    /// the next frame and are therefore not stored.
    pub fn serialize(&self) -> Vec<u8> {
        let mut writer = crate::state::StateWriter::with_capacity(Self::STATE_SIZE);
        writer.u64(self.disc.as_ref().map_or(0, crate::DiscImage::fingerprint));
        writer.u64(self.bus.firmware_fingerprint());
        writer.u32(self.display_width as u32);
        writer.u32(self.display_height as u32);
        writer.u64(self.frame_index);
        writer.u32_slots(&self.framebuffer, DISPLAY_WIDTH * DISPLAY_HEIGHT);
        self.input.save_state(&mut writer);
        self.cpu.save_state(&mut writer);
        self.bus.save_state(&mut writer);
        writer.into_bytes()
    }

    /// Restore a state produced by [`Emulator::serialize`]. The state is parsed
    /// into scratch copies first, so a rejected state leaves the running
    /// machine untouched.
    pub fn deserialize(&mut self, bytes: &[u8]) -> Result<(), EmulatorError> {
        let mut reader = crate::state::StateReader::new(bytes);
        let disc = reader.u64()?;
        let firmware = reader.u64()?;
        if firmware != self.bus.firmware_fingerprint() {
            return Err(EmulatorError::InvalidSaveState(
                "the state was written with different firmware",
            ));
        }
        if disc != self.disc.as_ref().map_or(0, crate::DiscImage::fingerprint) {
            return Err(EmulatorError::InvalidSaveState(
                "the state was written for different content",
            ));
        }
        let display_width = reader.u32()? as usize;
        let display_height = reader.u32()? as usize;
        if display_width == 0
            || display_height == 0
            || display_width > DISPLAY_WIDTH
            || display_height > DISPLAY_HEIGHT
            || !display_width.is_multiple_of(2)
        {
            return Err(EmulatorError::InvalidSaveState(
                "the stored frame geometry is not supported",
            ));
        }
        let frame_index = reader.u64()?;
        let mut framebuffer: Vec<u32> = reader.u32_slots(DISPLAY_WIDTH * DISPLAY_HEIGHT)?;
        framebuffer.truncate(display_width * display_height);
        let mut input = self.input;
        input.load_state(&mut reader)?;
        let mut cpu = self.cpu.clone();
        cpu.load_state(&mut reader)?;
        let mut bus = self.bus.clone();
        bus.load_state(&mut reader)?;

        self.cpu = cpu;
        self.bus = bus;
        self.framebuffer = framebuffer;
        self.audio_samples.clear();
        self.uart_output.clear();
        self.input = input;
        self.frame_index = frame_index;
        self.display_width = display_width;
        self.display_height = display_height;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BIOS_ROM_SIZE, INTERNAL_ROM_SIZE};

    fn test_firmware() -> Firmware {
        let mut internal = vec![0; INTERNAL_ROM_SIZE];
        let mut bios = vec![0; BIOS_ROM_SIZE];
        internal[INTERNAL_ROM_SIZE - 1] = 1;
        bios[BIOS_ROM_SIZE - 1] = 1;
        Firmware::from_parts(&internal, &bios).unwrap()
    }

    fn pack32(instruction: u32) -> u32 {
        0x8000_8000 | (instruction & 0x7fff) | ((instruction & 0x3fff_8000) << 1)
    }

    #[test]
    fn reset_restores_frontend_visible_state() {
        let mut emulator = Emulator::new(test_firmware());
        emulator.set_input(InputState {
            controllers: [
                crate::ControllerState {
                    buttons: 1,
                    analog_x: 10,
                    analog_y: 20,
                },
                crate::ControllerState::default(),
            ],
        });
        emulator.reset().unwrap();

        assert_eq!(emulator.frame_index(), 0);
        assert_eq!(emulator.framebuffer().len(), 320 * 240);
        assert!(emulator.framebuffer().iter().all(|pixel| *pixel == 0));
        assert!(emulator.audio_samples().is_empty());
    }

    #[test]
    fn serialized_state_has_the_reported_size() {
        let emulator = Emulator::new(test_firmware());
        let state = emulator.serialize();

        assert_eq!(state.len(), emulator.state_size());
    }

    #[test]
    fn save_state_round_trips_the_machine() {
        let mut emulator = Emulator::new(test_firmware());
        emulator.run_instructions(4).unwrap();
        let saved = emulator.serialize();

        emulator.run_instructions(4).unwrap();
        assert_ne!(emulator.serialize(), saved);

        emulator.deserialize(&saved).unwrap();
        assert_eq!(emulator.serialize(), saved);
    }

    #[test]
    fn state_from_other_firmware_is_rejected() {
        let mut emulator = Emulator::new(test_firmware());
        let mut saved = emulator.serialize();
        saved[8] ^= 0xff;

        assert_eq!(
            emulator.deserialize(&saved),
            Err(EmulatorError::InvalidSaveState(
                "the state was written with different firmware"
            ))
        );
    }

    #[test]
    fn truncated_state_is_rejected() {
        let mut emulator = Emulator::new(test_firmware());
        let saved = emulator.serialize();

        assert_eq!(
            emulator.deserialize(&saved[..saved.len() - 1]),
            Err(EmulatorError::InvalidSaveState(
                "the state buffer is truncated"
            ))
        );
    }

    #[test]
    fn run_frame_advances_video_timing() {
        let mut emulator = Emulator::new(test_firmware());
        emulator.run_frame().unwrap();

        assert_eq!(emulator.frame_index(), 1);
        assert_eq!(emulator.display_size(), (320, 240));
        assert_eq!(emulator.framebuffer().len(), 320 * 240);
    }

    #[test]
    fn runs_a_deterministic_instruction_budget() {
        let ldi_r4_7 = (1 << 25) | (4 << 20) | (6 << 17) | (7 << 1);
        let addi_r4_1 = (1 << 25) | (4 << 20) | (1 << 1);
        let mut internal = vec![0; INTERNAL_ROM_SIZE];
        let mut bios = vec![0; BIOS_ROM_SIZE];
        internal[INTERNAL_ROM_SIZE - 1] = 1;
        bios[0..4].copy_from_slice(&pack32(ldi_r4_7).to_le_bytes());
        bios[4..8].copy_from_slice(&pack32(addi_r4_1).to_le_bytes());
        let firmware = Firmware::from_parts(&internal, &bios).unwrap();
        let mut emulator = Emulator::new(firmware);

        let report = emulator.run_instructions(2).unwrap();

        assert_eq!(report.instructions, 2);
        assert_eq!(report.start_pc, 0x9f00_0000);
        assert_eq!(report.end_pc, 0x9f00_0008);
        assert_eq!(report.cycles, 12);
        assert_eq!(report.exceptions, 0);
        assert_eq!(emulator.cpu().register(4), Some(8));
    }
}
