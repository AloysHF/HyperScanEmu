use crate::{
    Bus, DiscImage, EmulatorError, Firmware, InputState, Score7, StepOutcome,
    CYCLES_PER_INSTRUCTION_ESTIMATE,
};

pub const DISPLAY_WIDTH: usize = 640;
pub const DISPLAY_HEIGHT: usize = 480;
pub const EMULATION_STRATEGY: &str = "lle";

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
    framebuffer: Vec<u32>,
    audio_samples: Vec<i16>,
    input: InputState,
    frame_index: u64,
}
impl Emulator {
    pub fn new(firmware: Firmware) -> Self {
        Self {
            cpu: Score7::new(),
            bus: Bus::new(firmware),
            disc: None,
            framebuffer: vec![0; DISPLAY_WIDTH * DISPLAY_HEIGHT],
            audio_samples: Vec::new(),
            input: InputState::default(),
            frame_index: 0,
        }
    }

    pub fn reset(&mut self) {
        self.cpu.reset();
        self.bus.reset();
        self.framebuffer.fill(0);
        self.audio_samples.clear();
        self.input = InputState::default();
        self.frame_index = 0;
    }

    pub fn set_input(&mut self, input: InputState) {
        self.input = input;
        self.bus.set_input(input);
    }

    pub fn attach_disc(&mut self, disc: DiscImage) {
        self.disc = Some(disc);
    }

    pub fn eject_disc(&mut self) -> Option<DiscImage> {
        self.disc.take()
    }

    pub fn disc(&self) -> Option<&DiscImage> {
        self.disc.as_ref()
    }

    pub fn cpu(&self) -> &Score7 {
        &self.cpu
    }

    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    pub fn step(&mut self) -> Result<StepOutcome, EmulatorError> {
        let outcome = self.cpu.step(&mut self.bus)?;
        if !matches!(outcome, StepOutcome::Exception { width: 0, .. }) {
            self.bus.tick(CYCLES_PER_INSTRUCTION_ESTIMATE)?;
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
        Err(EmulatorError::ExecutionNotImplemented)
    }

    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    pub fn audio_samples(&self) -> &[i16] {
        &self.audio_samples
    }

    pub fn frame_index(&self) -> u64 {
        self.frame_index
    }

    pub fn firmware_fingerprint(&self) -> u64 {
        self.bus.firmware_fingerprint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BIOS_ROM_SIZE, INTERNAL_ROM_SIZE};

    fn test_firmware() -> Firmware {
        let mut internal = vec![0xff; INTERNAL_ROM_SIZE];
        let mut bios = vec![0xff; BIOS_ROM_SIZE];
        internal[0] = 0;
        bios[0] = 0;
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
        emulator.reset();

        assert_eq!(emulator.frame_index(), 0);
        assert!(emulator.framebuffer().iter().all(|pixel| *pixel == 0));
        assert!(emulator.audio_samples().is_empty());
    }

    #[test]
    fn execution_gap_remains_explicit() {
        let mut emulator = Emulator::new(test_firmware());
        assert_eq!(
            emulator.run_frame(),
            Err(EmulatorError::ExecutionNotImplemented)
        );
    }

    #[test]
    fn runs_a_deterministic_instruction_budget() {
        let ldi_r4_7 = (1 << 25) | (4 << 20) | (6 << 17) | (7 << 1);
        let addi_r4_1 = (1 << 25) | (4 << 20) | (1 << 1);
        let mut internal = vec![0; INTERNAL_ROM_SIZE];
        let mut bios = vec![0; BIOS_ROM_SIZE];
        internal[0..4].copy_from_slice(&pack32(ldi_r4_7).to_le_bytes());
        internal[4..8].copy_from_slice(&pack32(addi_r4_1).to_le_bytes());
        bios[0] = 1;
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
