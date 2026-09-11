use crate::{EmulatorError, Firmware, InputState};

pub const DISPLAY_WIDTH: usize = 640;
pub const DISPLAY_HEIGHT: usize = 480;
pub const EMULATION_STRATEGY: &str = "lle";

#[derive(Debug)]
pub struct Emulator {
    firmware: Firmware,
    framebuffer: Vec<u32>,
    audio_samples: Vec<i16>,
    input: InputState,
    frame_index: u64,
}
impl Emulator {
    pub fn new(firmware: Firmware) -> Self {
        Self {
            firmware,
            framebuffer: vec![0; DISPLAY_WIDTH * DISPLAY_HEIGHT],
            audio_samples: Vec::new(),
            input: InputState::default(),
            frame_index: 0,
        }
    }

    pub fn reset(&mut self) {
        self.framebuffer.fill(0);
        self.audio_samples.clear();
        self.input = InputState::default();
        self.frame_index = 0;
    }

    pub fn set_input(&mut self, input: InputState) {
        self.input = input;
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
        self.firmware.fingerprint()
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

    #[test]
    fn reset_restores_frontend_visible_state() {
        let mut emulator = Emulator::new(test_firmware());
        emulator.set_input(InputState {
            buttons: 1,
            pointer_x: 10,
            pointer_y: 20,
            pointer_pressed: true,
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
}
