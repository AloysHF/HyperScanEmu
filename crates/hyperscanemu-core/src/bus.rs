use crate::{EmulatorError, Firmware, Spg290Devices};

pub const ADDRESS_MASK: u32 = 0x1fff_ffff;
pub const DRAM_SIZE: usize = 0x0100_0000;
pub const INTERNAL_SRAM_SIZE: usize = 0x4000;

const DRAM_END: u32 = 0x07ff_ffff;
const MMIO_START: u32 = 0x0800_0000;
const MMIO_END: u32 = 0x0824_ffff;
const INTERNAL_SRAM_START: u32 = 0x0a00_0000;
const INTERNAL_ROM_START: u32 = 0x0b00_0000;
const EXTERNAL_ROM_START: u32 = 0x1000_0000;
const EXTERNAL_ROM_END: u32 = 0x1fff_ffff;
const BOOT_WINDOW_START: u32 = 0x1f00_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootSource {
    InternalRom,
    ExternalRom,
}

#[derive(Debug, Clone)]
pub struct Bus {
    dram: Box<[u8]>,
    internal_sram: Box<[u8]>,
    firmware: Firmware,
    boot_source: BootSource,
    devices: Spg290Devices,
}

impl Bus {
    pub fn new(firmware: Firmware) -> Self {
        Self {
            dram: vec![0; DRAM_SIZE].into_boxed_slice(),
            internal_sram: vec![0; INTERNAL_SRAM_SIZE].into_boxed_slice(),
            firmware,
            boot_source: BootSource::InternalRom,
            devices: Spg290Devices::new(),
        }
    }

    pub fn reset(&mut self) {
        self.dram.fill(0);
        self.internal_sram.fill(0);
        self.boot_source = BootSource::InternalRom;
        self.devices.reset();
    }

    pub fn boot_source(&self) -> BootSource {
        self.boot_source
    }

    pub fn set_boot_source(&mut self, source: BootSource) {
        self.boot_source = source;
    }

    pub fn read_u8(&self, address: u32) -> Result<u8, EmulatorError> {
        let address = address & ADDRESS_MASK;
        match address {
            0..=DRAM_END => Ok(self.dram[address as usize % DRAM_SIZE]),
            MMIO_START..=MMIO_END => Err(memory_fault("read", address, 1)),
            INTERNAL_SRAM_START..=0x0a00_3fff => {
                Ok(self.internal_sram[(address - INTERNAL_SRAM_START) as usize])
            }
            INTERNAL_ROM_START..=0x0b00_7fff => {
                Ok(self.firmware.internal_rom()[(address - INTERNAL_ROM_START) as usize])
            }
            EXTERNAL_ROM_START..=EXTERNAL_ROM_END => self.read_external_window(address),
            _ => Err(memory_fault("read", address, 1)),
        }
    }

    pub fn read_u16(&self, address: u32) -> Result<u16, EmulatorError> {
        ensure_aligned(address, 2, "read")?;
        let bytes = [self.read_u8(address)?, self.read_u8(address + 1)?];
        Ok(u16::from_le_bytes(bytes))
    }

    pub fn read_u32(&self, address: u32) -> Result<u32, EmulatorError> {
        ensure_aligned(address, 4, "read")?;
        let masked = address & ADDRESS_MASK;
        if (MMIO_START..=MMIO_END).contains(&masked) {
            return self.devices.read_u32(masked);
        }
        let bytes = [
            self.read_u8(address)?,
            self.read_u8(address + 1)?,
            self.read_u8(address + 2)?,
            self.read_u8(address + 3)?,
        ];
        Ok(u32::from_le_bytes(bytes))
    }

    pub fn write_u8(&mut self, address: u32, value: u8) -> Result<(), EmulatorError> {
        let address = address & ADDRESS_MASK;
        match address {
            0..=DRAM_END => {
                self.dram[address as usize % DRAM_SIZE] = value;
                Ok(())
            }
            INTERNAL_SRAM_START..=0x0a00_3fff => {
                self.internal_sram[(address - INTERNAL_SRAM_START) as usize] = value;
                Ok(())
            }
            _ => Err(memory_fault("write", address, 1)),
        }
    }

    pub fn write_u16(&mut self, address: u32, value: u16) -> Result<(), EmulatorError> {
        ensure_aligned(address, 2, "write")?;
        let bytes = value.to_le_bytes();
        self.write_u8(address, bytes[0])?;
        self.write_u8(address + 1, bytes[1])
    }

    pub fn write_u32(&mut self, address: u32, value: u32) -> Result<(), EmulatorError> {
        ensure_aligned(address, 4, "write")?;
        let masked = address & ADDRESS_MASK;
        if (MMIO_START..=MMIO_END).contains(&masked) {
            return self.devices.write_u32(masked, value);
        }
        let bytes = value.to_le_bytes();
        self.write_u8(address, bytes[0])?;
        self.write_u8(address + 1, bytes[1])?;
        self.write_u8(address + 2, bytes[2])?;
        self.write_u8(address + 3, bytes[3])
    }

    pub fn firmware_fingerprint(&self) -> u64 {
        self.firmware.fingerprint()
    }

    pub fn tick(&mut self, cpu_cycles: u64) -> Result<(), EmulatorError> {
        self.devices.tick(cpu_cycles)
    }

    pub fn take_pending_interrupts(&mut self) -> u64 {
        self.devices.take_pending_interrupts()
    }

    fn read_external_window(&self, address: u32) -> Result<u8, EmulatorError> {
        if self.boot_source == BootSource::InternalRom && address >= BOOT_WINDOW_START {
            let offset = (address - BOOT_WINDOW_START) as usize;
            return Ok(self.firmware.internal_rom()[offset % self.firmware.internal_rom().len()]);
        }

        let offset = (address - EXTERNAL_ROM_START) as usize;
        Ok(self.firmware.bios_rom()[offset % self.firmware.bios_rom().len()])
    }
}

fn ensure_aligned(address: u32, width: u8, access: &'static str) -> Result<(), EmulatorError> {
    if !address.is_multiple_of(u32::from(width)) {
        return Err(memory_fault(access, address & ADDRESS_MASK, width));
    }
    Ok(())
}

fn memory_fault(access: &'static str, address: u32, width: u8) -> EmulatorError {
    EmulatorError::MemoryFault {
        access,
        address,
        width,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BIOS_ROM_SIZE, INTERNAL_ROM_SIZE};

    fn test_bus() -> Bus {
        let mut internal = vec![0; INTERNAL_ROM_SIZE];
        let mut bios = vec![0; BIOS_ROM_SIZE];
        internal[0..4].copy_from_slice(&0x1122_3344_u32.to_le_bytes());
        bios[0..4].copy_from_slice(&0xaabb_ccdd_u32.to_le_bytes());
        Bus::new(Firmware::from_parts(&internal, &bios).unwrap())
    }

    #[test]
    fn dram_is_little_endian_and_mirrored() {
        let mut bus = test_bus();
        bus.write_u32(0x0000_0100, 0x4433_2211).unwrap();

        assert_eq!(bus.read_u8(0x0100_0100).unwrap(), 0x11);
        assert_eq!(bus.read_u16(0x0200_0100).unwrap(), 0x2211);
        assert_eq!(bus.read_u32(0x0700_0100).unwrap(), 0x4433_2211);
    }

    #[test]
    fn high_addresses_are_masked_to_the_hardware_bus() {
        let bus = test_bus();

        assert_eq!(bus.read_u32(0x8b00_0000).unwrap(), 0x1122_3344);
        assert_eq!(bus.read_u32(0x9000_0000).unwrap(), 0xaabb_ccdd);
    }

    #[test]
    fn reset_window_starts_on_internal_rom() {
        let mut bus = test_bus();

        assert_eq!(bus.read_u32(0x9f00_0000).unwrap(), 0x1122_3344);
        bus.set_boot_source(BootSource::ExternalRom);
        assert_eq!(bus.read_u32(0x9f00_0000).unwrap(), 0xaabb_ccdd);
    }

    #[test]
    fn rejects_rom_writes_unmapped_access_and_misalignment() {
        let mut bus = test_bus();

        assert_eq!(
            bus.write_u8(0x0b00_0000, 1),
            Err(memory_fault("write", 0x0b00_0000, 1))
        );
        assert_eq!(
            bus.read_u8(0x0830_0000),
            Err(memory_fault("read", 0x0830_0000, 1))
        );
        assert_eq!(
            bus.read_u32(0x0000_0002),
            Err(memory_fault("read", 0x0000_0002, 4))
        );
    }

    #[test]
    fn reset_clears_writable_memory_and_restores_boot_source() {
        let mut bus = test_bus();
        bus.write_u8(0x100, 0x55).unwrap();
        bus.write_u8(INTERNAL_SRAM_START, 0xaa).unwrap();
        bus.set_boot_source(BootSource::ExternalRom);

        bus.reset();

        assert_eq!(bus.read_u8(0x100).unwrap(), 0);
        assert_eq!(bus.read_u8(INTERNAL_SRAM_START).unwrap(), 0);
        assert_eq!(bus.boot_source(), BootSource::InternalRom);
    }
}
