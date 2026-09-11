use crate::EmulatorError;

pub const INTERNAL_ROM_SIZE: usize = 0x8_000;
pub const BIOS_ROM_SIZE: usize = 0x10_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Firmware {
    internal_rom: Box<[u8]>,
    bios_rom: Box<[u8]>,
    fingerprint: u64,
}

impl Firmware {
    pub fn from_parts(internal_rom: &[u8], bios_rom: &[u8]) -> Result<Self, EmulatorError> {
        validate_component("internal ROM", internal_rom, INTERNAL_ROM_SIZE)?;
        validate_component("BIOS ROM", bios_rom, BIOS_ROM_SIZE)?;

        let mut fingerprint = FNV_OFFSET_BASIS;
        fingerprint = fingerprint_bytes(fingerprint, internal_rom);
        fingerprint = fingerprint_bytes(fingerprint, bios_rom);

        Ok(Self {
            internal_rom: internal_rom.into(),
            bios_rom: bios_rom.into(),
            fingerprint,
        })
    }

    pub fn internal_rom(&self) -> &[u8] {
        &self.internal_rom
    }

    pub fn bios_rom(&self) -> &[u8] {
        &self.bios_rom
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

fn validate_component(
    component: &'static str,
    bytes: &[u8],
    expected: usize,
) -> Result<(), EmulatorError> {
    if bytes.len() != expected {
        return Err(EmulatorError::InvalidFirmwareSize {
            component,
            expected,
            actual: bytes.len(),
        });
    }

    if bytes.iter().all(|byte| *byte == bytes[0]) {
        return Err(EmulatorError::BlankFirmware { component });
    }

    Ok(())
}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

fn fingerprint_bytes(initial: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(initial, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_component(size: usize, marker: u8) -> Vec<u8> {
        let mut bytes = vec![0xff; size];
        bytes[size / 2] = marker;
        bytes
    }

    #[test]
    fn accepts_exact_non_blank_components() {
        let internal = valid_component(INTERNAL_ROM_SIZE, 0x42);
        let bios = valid_component(BIOS_ROM_SIZE, 0x24);
        let firmware = Firmware::from_parts(&internal, &bios).unwrap();

        assert_eq!(firmware.internal_rom(), internal);
        assert_eq!(firmware.bios_rom(), bios);
        assert_ne!(firmware.fingerprint(), 0);
    }

    #[test]
    fn rejects_wrong_component_size() {
        let internal = valid_component(INTERNAL_ROM_SIZE - 1, 0x42);
        let bios = valid_component(BIOS_ROM_SIZE, 0x24);

        assert_eq!(
            Firmware::from_parts(&internal, &bios),
            Err(EmulatorError::InvalidFirmwareSize {
                component: "internal ROM",
                expected: INTERNAL_ROM_SIZE,
                actual: INTERNAL_ROM_SIZE - 1,
            })
        );
    }

    #[test]
    fn rejects_uniform_dump() {
        let internal = vec![0xff; INTERNAL_ROM_SIZE];
        let bios = valid_component(BIOS_ROM_SIZE, 0x24);

        assert_eq!(
            Firmware::from_parts(&internal, &bios),
            Err(EmulatorError::BlankFirmware {
                component: "internal ROM",
            })
        );
    }

    #[test]
    fn fingerprint_covers_both_components() {
        let internal = valid_component(INTERNAL_ROM_SIZE, 0x42);
        let bios_a = valid_component(BIOS_ROM_SIZE, 0x24);
        let bios_b = valid_component(BIOS_ROM_SIZE, 0x25);

        let a = Firmware::from_parts(&internal, &bios_a).unwrap();
        let b = Firmware::from_parts(&internal, &bios_b).unwrap();

        assert_ne!(a.fingerprint(), b.fingerprint());
    }
}
