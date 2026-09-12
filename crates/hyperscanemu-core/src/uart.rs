const DATA: u32 = 0x00;
const ERROR_STATUS: u32 = 0x04;
const MODE_CONTROL: u32 = 0x08;
const BAUD_RATE: u32 = 0x0c;
const FIFO_STATUS: u32 = 0x10;

const RX_FIFO_EMPTY: u32 = 1 << 4;
const TX_FIFO_EMPTY: u32 = 1 << 7;

#[derive(Debug, Clone, Default)]
pub(crate) struct Uart {
    mode_control: u32,
    baud_rate: u32,
    output: Vec<u8>,
}

impl Uart {
    pub(crate) fn read(&self, offset: u32) -> Option<u32> {
        match offset {
            DATA => Some(u32::from(u8::MAX)),
            ERROR_STATUS => Some(0),
            MODE_CONTROL => Some(self.mode_control),
            BAUD_RATE => Some(self.baud_rate),
            FIFO_STATUS => Some(RX_FIFO_EMPTY | TX_FIFO_EMPTY),
            _ => None,
        }
    }

    pub(crate) fn write(&mut self, offset: u32, value: u32) -> Option<()> {
        match offset {
            DATA => self.output.push(value as u8),
            ERROR_STATUS => {}
            MODE_CONTROL => self.mode_control = value,
            BAUD_RATE => self.baud_rate = value,
            FIFO_STATUS => {}
            _ => return None,
        }
        Some(())
    }

    pub(crate) fn drain_output(&mut self, destination: &mut Vec<u8>) {
        destination.clear();
        destination.append(&mut self.output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transmitter_is_immediately_ready_and_captures_bytes() {
        let mut uart = Uart::default();
        uart.write(MODE_CONTROL, 0x1060).unwrap();
        uart.write(BAUD_RATE, 234).unwrap();
        uart.write(DATA, u32::from(b'A')).unwrap();
        let mut output = Vec::new();

        uart.drain_output(&mut output);

        assert_eq!(uart.read(MODE_CONTROL), Some(0x1060));
        assert_eq!(uart.read(BAUD_RATE), Some(234));
        assert_eq!(uart.read(FIFO_STATUS), Some(0x90));
        assert_eq!(output, b"A");
    }
}
