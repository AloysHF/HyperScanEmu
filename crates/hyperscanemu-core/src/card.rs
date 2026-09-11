use crate::EmulatorError;

pub const CARD_SIZE: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardImage {
    memory: [u8; CARD_SIZE],
    dirty: bool,
}

impl CardImage {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EmulatorError> {
        let memory = bytes
            .try_into()
            .map_err(|_| EmulatorError::InvalidCardSize {
                expected: CARD_SIZE,
                actual: bytes.len(),
            })?;
        Ok(Self {
            memory,
            dirty: false,
        })
    }

    pub fn bytes(&self) -> &[u8; CARD_SIZE] {
        &self.memory
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CardDevice {
    image: Option<CardImage>,
    line_state: bool,
    elapsed_cycles: u64,
    bit_position: u8,
    received_byte: u8,
    command: Vec<u8>,
    response: Vec<bool>,
    response_index: usize,
}

impl CardDevice {
    pub(crate) fn reset(&mut self) {
        self.line_state = false;
        self.elapsed_cycles = 0;
        self.bit_position = 0;
        self.received_byte = 0;
        self.command.clear();
        self.response.clear();
        self.response_index = 0;
    }

    pub(crate) fn insert(&mut self, image: CardImage) -> Option<CardImage> {
        let previous = self.image.replace(image);
        self.reset();
        previous
    }

    pub(crate) fn eject(&mut self) -> Option<CardImage> {
        let image = self.image.take();
        self.reset();
        image
    }

    pub(crate) fn tick(&mut self, cpu_cycles: u64) {
        self.elapsed_cycles = self.elapsed_cycles.saturating_add(cpu_cycles);
    }

    pub(crate) fn write_line(&mut self, state: bool) {
        if self.line_state == state || self.image.is_none() {
            return;
        }

        self.line_state = state;
        let pulse_cycles = std::mem::take(&mut self.elapsed_cycles);
        if state {
            return;
        }

        let radio_clocks = pulse_cycles.saturating_mul(113) / 900;
        if radio_clocks < 1_000 {
            if radio_clocks >= 500 {
                self.received_byte |= 1 << self.bit_position;
            } else {
                self.received_byte &= !(1 << self.bit_position);
            }
            self.bit_position += 1;

            let complete =
                (self.command.is_empty() && self.bit_position == 7) || self.bit_position == 8;
            if complete {
                if self.command.is_empty() {
                    self.received_byte &= 0x7f;
                }
                self.command.push(self.received_byte);
                self.received_byte = 0;
                self.bit_position = 0;
                self.check_command();
            }
        } else if radio_clocks < 5_000 {
            self.bit_position = 0;
            self.received_byte = 0;
            if self.response_index <= self.response.len() * 2 {
                self.response_index += 1;
            }
        }
    }

    pub(crate) fn read_line(&self) -> bool {
        let bit_index = self.response_index >> 1;
        self.image.is_some()
            && bit_index < self.response.len()
            && (self.response[bit_index] ^ (self.response_index & 1 != 0))
    }

    fn check_command(&mut self) {
        let Some(opcode) = self.command.first().copied() else {
            return;
        };

        if self.command.len() == 1 && matches!(opcode, 0x26 | 0x52) {
            self.command.clear();
            self.make_response(&[0, 0], false);
            return;
        }
        if self.command.len() != 9 {
            return;
        }

        let mut response = Vec::new();
        match opcode {
            0x78 => {
                response.extend_from_slice(&[0x11, 0]);
                response.extend_from_slice(&self.image.as_ref().unwrap().memory[..4]);
                self.command.clear();
                self.make_response(&response, true);
            }
            0x01 => {
                let address = usize::from(self.command[1]) % CARD_SIZE;
                response.extend_from_slice(&[
                    address as u8,
                    self.image.as_ref().unwrap().memory[address],
                ]);
                self.command.clear();
                self.make_response(&response, true);
            }
            0x00 => {
                response.extend_from_slice(&[0x11, 0]);
                response.extend_from_slice(&self.image.as_ref().unwrap().memory);
                self.command.clear();
                self.make_response(&response, true);
            }
            0x53 | 0x1a => {
                let address = usize::from(self.command[1]) % CARD_SIZE;
                let value = self.command[2];
                let image = self.image.as_mut().unwrap();
                let new_value = if opcode == 0x53 {
                    value
                } else {
                    image.memory[address] | value
                };
                if image.memory[address] != new_value {
                    image.memory[address] = new_value;
                    image.dirty = true;
                }
                response.extend_from_slice(&[address as u8, value]);
                self.command.clear();
                self.make_response(&response, true);
            }
            _ => {}
        }
    }

    fn make_response(&mut self, data: &[u8], add_crc: bool) {
        self.response_index = 0;
        self.response.clear();
        self.add_response_byte(0, false);
        self.add_response_byte(0x80, false);
        for byte in data {
            self.add_response_byte(*byte, true);
        }
        if add_crc {
            let crc = calculate_crc(data);
            self.add_response_byte(crc as u8, true);
            self.add_response_byte((crc >> 8) as u8, true);
        }
    }

    fn add_response_byte(&mut self, byte: u8, add_parity: bool) {
        let mut parity = true;
        for bit in 0..8 {
            let value = byte & (1 << bit) != 0;
            self.response.push(value);
            parity ^= value;
        }
        if add_parity {
            self.response.push(parity);
        }
    }
}

fn calculate_crc(data: &[u8]) -> u16 {
    let mut crc = 0xffff_u16;
    for byte in data {
        let mut value = *byte ^ crc as u8;
        value ^= value << 4;
        crc = (crc >> 8)
            ^ (u16::from(value) << 8)
            ^ (u16::from(value) << 3)
            ^ (u16::from(value) >> 4);
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pulse(card: &mut CardDevice, bit: bool) {
        card.write_line(true);
        card.tick(if bit { 5_000 } else { 1_000 });
        card.write_line(false);
    }

    fn send_command(card: &mut CardDevice, command: &[u8]) {
        for bit in 0..7 {
            pulse(card, command[0] & (1 << bit) != 0);
        }
        for byte in &command[1..] {
            for bit in 0..8 {
                pulse(card, byte & (1 << bit) != 0);
            }
        }
    }

    #[test]
    fn card_images_have_a_strict_size() {
        assert_eq!(
            CardImage::from_bytes(&[0; CARD_SIZE - 1]),
            Err(EmulatorError::InvalidCardSize {
                expected: CARD_SIZE,
                actual: CARD_SIZE - 1,
            })
        );
        assert!(CardImage::from_bytes(&[0; CARD_SIZE]).is_ok());
    }

    #[test]
    fn read_command_builds_framed_response() {
        let mut bytes = [0; CARD_SIZE];
        bytes[10] = 0xa5;
        let mut card = CardDevice::default();
        card.insert(CardImage::from_bytes(&bytes).unwrap());
        let mut command = [0; 9];
        command[0] = 0x01;
        command[1] = 10;

        send_command(&mut card, &command);

        assert_eq!(card.response.len(), 52);
        assert!(!card.response[0]);
        assert!(card.response[15]);
        let data_start = 16;
        let address = (0..8).fold(0_u8, |value, bit| {
            value | (u8::from(card.response[data_start + bit]) << bit)
        });
        let value_start = data_start + 9;
        let value = (0..8).fold(0_u8, |value, bit| {
            value | (u8::from(card.response[value_start + bit]) << bit)
        });
        assert_eq!(address, 10);
        assert_eq!(value, 0xa5);
    }

    #[test]
    fn write_commands_modify_and_dirty_the_card() {
        let mut card = CardDevice::default();
        card.insert(CardImage::from_bytes(&[0; CARD_SIZE]).unwrap());
        let mut command = [0; 9];
        command[0] = 0x53;
        command[1] = 12;
        command[2] = 0x55;

        send_command(&mut card, &command);
        let image = card.eject().unwrap();

        assert_eq!(image.bytes()[12], 0x55);
        assert!(image.is_dirty());
    }

    #[test]
    fn crc_matches_protocol_vector() {
        assert_eq!(calculate_crc(&[10, 0xa5]), 0x0090);
    }
}
