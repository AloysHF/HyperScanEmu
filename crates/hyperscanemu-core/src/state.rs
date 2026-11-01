//! Binary encoding used by save states.
//!
//! The layout is fixed-size: every collection that can appear in the machine
//! state is written into a fixed slot, so `Emulator::state_size()` stays stable
//! for as long as the content is loaded (the libretro frontend reads it once).
//! The envelope (magic and format version) lives in the libretro adapter.

use crate::EmulatorError;

/// Upper bound for queues and command buffers that are copied verbatim.
pub(crate) const COLLECTION_SLOTS: usize = 2048;

/// Write primitives into the growing state buffer.
#[derive(Debug, Default)]
pub(crate) struct StateWriter {
    bytes: Vec<u8>,
}

impl StateWriter {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(crate) fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    pub(crate) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write exactly `slots` bytes, padding or truncating `value`.
    pub(crate) fn byte_slot(&mut self, value: &[u8], slots: usize) {
        let copied = value.len().min(slots);
        self.bytes.extend_from_slice(&value[..copied]);
        self.bytes.resize(self.bytes.len() + slots - copied, 0);
    }

    /// Write exactly `slots` little-endian words, padding or truncating `value`.
    pub(crate) fn u32_slots(&mut self, value: &[u32], slots: usize) {
        for index in 0..slots {
            self.u32(value.get(index).copied().unwrap_or(0));
        }
    }
}

/// Read primitives back, rejecting truncated buffers.
pub(crate) struct StateReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> StateReader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], EmulatorError> {
        let end = self
            .offset
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(EmulatorError::InvalidSaveState(
                "the state buffer is truncated",
            ))?;
        let slice = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, EmulatorError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn bool(&mut self) -> Result<bool, EmulatorError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(EmulatorError::InvalidSaveState(
                "a flag is neither true nor false",
            )),
        }
    }

    pub(crate) fn u16(&mut self) -> Result<u16, EmulatorError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, EmulatorError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, EmulatorError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub(crate) fn bytes<const N: usize>(&mut self) -> Result<[u8; N], EmulatorError> {
        let mut value = [0_u8; N];
        value.copy_from_slice(self.take(N)?);
        Ok(value)
    }

    pub(crate) fn byte_slot(&mut self, slots: usize) -> Result<&'a [u8], EmulatorError> {
        self.take(slots)
    }

    pub(crate) fn u32_slots(&mut self, slots: usize) -> Result<Vec<u32>, EmulatorError> {
        (0..slots).map(|_| self.u32()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_round_trip() {
        let mut writer = StateWriter::with_capacity(64);
        writer.u8(0x12);
        writer.bool(true);
        writer.u16(0x3456);
        writer.u32(0x789a_bcde);
        writer.u64(0x0123_4567_89ab_cdef);

        let bytes = writer.into_bytes();
        let mut reader = StateReader::new(&bytes);

        assert_eq!(reader.u8().unwrap(), 0x12);
        assert!(reader.bool().unwrap());
        assert_eq!(reader.u16().unwrap(), 0x3456);
        assert_eq!(reader.u32().unwrap(), 0x789a_bcde);
        assert_eq!(reader.u64().unwrap(), 0x0123_4567_89ab_cdef);
    }

    #[test]
    fn slots_are_padded_and_truncated() {
        let mut writer = StateWriter::default();
        writer.byte_slot(&[1, 2, 3], 5);
        writer.u32_slots(&[7, 8, 9], 2);
        let bytes = writer.into_bytes();

        assert_eq!(bytes.len(), 13);
        assert_eq!(&bytes[..5], &[1, 2, 3, 0, 0]);
        let mut reader = StateReader::new(&bytes);
        assert_eq!(reader.byte_slot(5).unwrap(), &[1, 2, 3, 0, 0]);
        assert_eq!(reader.u32_slots(2).unwrap(), vec![7, 8]);
    }

    #[test]
    fn truncated_buffers_are_rejected() {
        let bytes = [0_u8; 3];
        let mut reader = StateReader::new(&bytes);

        assert_eq!(
            reader.u32(),
            Err(EmulatorError::InvalidSaveState(
                "the state buffer is truncated"
            ))
        );
    }

    #[test]
    fn invalid_flags_are_rejected() {
        let bytes = [2_u8];
        let mut reader = StateReader::new(&bytes);

        assert_eq!(
            reader.bool(),
            Err(EmulatorError::InvalidSaveState(
                "a flag is neither true nor false"
            ))
        );
    }
}
