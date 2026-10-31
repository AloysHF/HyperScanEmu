#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ControllerButton {
    Blue = 1 << 0,
    Start = 1 << 1,
    Select = 1 << 2,
    LeftShoulder = 1 << 3,
    RightShoulder = 1 << 4,
    LeftTrigger = 1 << 5,
    RightTrigger = 1 << 6,
    Yellow = 1 << 7,
    Red = 1 << 8,
    Green = 1 << 9,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControllerState {
    pub buttons: u16,
    pub analog_x: u8,
    pub analog_y: u8,
}

impl Default for ControllerState {
    fn default() -> Self {
        Self {
            buttons: 0,
            analog_x: 0x7f,
            analog_y: 0x7f,
        }
    }
}

impl ControllerState {
    pub fn button_pressed(self, button: ControllerButton) -> bool {
        self.buttons & button as u16 != 0
    }

    pub fn set_button(&mut self, button: ControllerButton, pressed: bool) {
        if pressed {
            self.buttons |= button as u16;
        } else {
            self.buttons &= !(button as u16);
        }
    }

    pub(crate) fn packet(self) -> [u8; 4] {
        let mut first = 0;
        for (button, mask) in [
            (ControllerButton::Blue, 1 << 0),
            (ControllerButton::Start, 1 << 1),
            (ControllerButton::Select, 1 << 2),
            (ControllerButton::LeftShoulder, 1 << 3),
            (ControllerButton::RightShoulder, 1 << 4),
            (ControllerButton::LeftTrigger, 1 << 5),
            (ControllerButton::RightTrigger, 1 << 6),
        ] {
            if self.button_pressed(button) {
                first |= mask;
            }
        }

        let mut second = 0;
        if self.button_pressed(ControllerButton::Yellow) {
            second |= 1 << 5;
        }
        if self.button_pressed(ControllerButton::Red) {
            second |= 1 << 6;
        }
        if self.button_pressed(ControllerButton::Green) {
            second |= 1 << 7;
        }

        [first, second, self.analog_y, self.analog_x]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputState {
    pub controllers: [ControllerState; 2],
}

impl ControllerState {
    pub(crate) const STATE_SIZE: usize = 2 + 1 + 1;

    pub(crate) fn save_state(&self, writer: &mut crate::state::StateWriter) {
        writer.u16(self.buttons);
        writer.u8(self.analog_x);
        writer.u8(self.analog_y);
    }

    pub(crate) fn load_state(
        &mut self,
        reader: &mut crate::state::StateReader,
    ) -> Result<(), crate::EmulatorError> {
        self.buttons = reader.u16()?;
        self.analog_x = reader.u8()?;
        self.analog_y = reader.u8()?;
        Ok(())
    }
}

impl InputState {
    pub(crate) const STATE_SIZE: usize = 2 * ControllerState::STATE_SIZE;

    pub(crate) fn save_state(&self, writer: &mut crate::state::StateWriter) {
        for controller in &self.controllers {
            controller.save_state(writer);
        }
    }

    pub(crate) fn load_state(
        &mut self,
        reader: &mut crate::state::StateReader,
    ) -> Result<(), crate::EmulatorError> {
        for controller in &mut self.controllers {
            controller.load_state(reader)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_packet_matches_wire_layout() {
        let mut controller = ControllerState::default();
        controller.set_button(ControllerButton::Blue, true);
        controller.set_button(ControllerButton::RightTrigger, true);
        controller.set_button(ControllerButton::Yellow, true);
        controller.set_button(ControllerButton::Green, true);
        controller.analog_x = 0x12;
        controller.analog_y = 0x34;

        assert_eq!(controller.packet(), [0x41, 0xa0, 0x34, 0x12]);
    }

    #[test]
    fn controller_axes_default_to_center() {
        let input = InputState::default();

        assert!(input
            .controllers
            .iter()
            .all(|controller| { controller.analog_x == 0x7f && controller.analog_y == 0x7f }));
    }
}
