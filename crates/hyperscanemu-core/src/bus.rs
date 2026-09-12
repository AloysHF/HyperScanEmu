use crate::{
    video::rgb565_to_xrgb8888, CardImage, DiscImage, EmulatorError, Firmware, InputState,
    Spg290Devices,
};

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
            boot_source: BootSource::ExternalRom,
            devices: Spg290Devices::new(),
        }
    }

    pub fn reset(&mut self) {
        self.dram.fill(0);
        self.internal_sram.fill(0);
        self.boot_source = BootSource::ExternalRom;
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
            MMIO_START..=MMIO_END => {
                let shift = (address & 3) * 8;
                Ok((self.devices.read_u32(address & !3)? >> shift) as u8)
            }
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
        if (MMIO_START..=MMIO_END).contains(&address) {
            let aligned = address & !3;
            let shift = (address & 3) * 8;
            let mask = 0xff_u32 << shift;
            let current = self.devices.read_u32(aligned)?;
            return self.write_mmio_u32(aligned, (current & !mask) | (u32::from(value) << shift));
        }
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
        let masked = address & ADDRESS_MASK;
        if (MMIO_START..=MMIO_END).contains(&masked) {
            let aligned = masked & !3;
            let shift = (masked & 2) * 8;
            let mask = 0xffff_u32 << shift;
            let current = self.devices.read_u32(aligned)?;
            return self.write_mmio_u32(aligned, (current & !mask) | (u32::from(value) << shift));
        }
        let bytes = value.to_le_bytes();
        self.write_u8(address, bytes[0])?;
        self.write_u8(address + 1, bytes[1])
    }

    pub fn write_u32(&mut self, address: u32, value: u32) -> Result<(), EmulatorError> {
        ensure_aligned(address, 4, "write")?;
        let masked = address & ADDRESS_MASK;
        if (MMIO_START..=MMIO_END).contains(&masked) {
            return self.write_mmio_u32(masked, value);
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

    fn write_mmio_u32(&mut self, address: u32, value: u32) -> Result<(), EmulatorError> {
        self.devices.write_u32(address, value)?;
        if self.devices.external_rom_selected() {
            self.boot_source = BootSource::ExternalRom;
        }
        Ok(())
    }

    pub fn tick(&mut self, cpu_cycles: u64) -> Result<(), EmulatorError> {
        self.devices.tick(cpu_cycles)?;
        while let Some(request) = self.devices.take_audio_dma_request() {
            let left = self.read_u16(request.left_address)?;
            let right = self.read_u16(request.right_address)?;
            self.devices.complete_audio_dma(left, right);
        }
        Ok(())
    }

    pub fn take_pending_interrupts(&mut self) -> u64 {
        self.devices.take_pending_interrupts()
    }

    pub fn set_input(&mut self, input: InputState) {
        self.devices.set_input(input);
    }

    pub fn insert_card(&mut self, card: CardImage) -> Option<CardImage> {
        self.devices.insert_card(card)
    }

    pub fn eject_card(&mut self) -> Option<CardImage> {
        self.devices.eject_card()
    }

    pub fn set_disc(&mut self, disc: Option<&DiscImage>) {
        self.devices.set_disc(disc.map(DiscImage::sector_count));
    }

    pub fn cd_command_trace(&self) -> Vec<crate::CdCommandTrace> {
        self.devices.cd_command_trace()
    }

    pub(crate) fn set_execution_context(&mut self, pc: u32, link: u32) {
        self.devices.set_execution_context(pc, link);
    }

    pub fn cd_servo_state(&self) -> crate::CdServoState {
        self.devices.cd_servo_state()
    }

    pub fn service_cd(&mut self, disc: &DiscImage) -> Result<(), EmulatorError> {
        while let Some(request) = self.devices.take_cd_dma_request() {
            let sector = disc.read_raw_sector(request.lba)?;
            let mut pointer = request.buffer_pointer;
            for index in 0..request.sector_size {
                let byte = sector.get(index).copied().unwrap_or(0);
                self.write_u8(pointer, byte)?;
                pointer = pointer.wrapping_add(1);
                if pointer > request.buffer_end {
                    pointer = request.buffer_start;
                }
            }
            self.devices.complete_cd_dma(pointer);
        }
        Ok(())
    }

    pub fn cycles_until_frame_end(&self) -> u64 {
        self.devices.cycles_until_frame_end()
    }

    pub fn render_frame(&self, output: &mut Vec<u32>) -> Result<(usize, usize), EmulatorError> {
        let state = self.devices.direct_frame_state();
        output.resize(state.width * state.height, 0xff00_0000);
        output.fill(0xff00_0000);
        self.render_ppu(output, state.width, state.height)?;

        if state.enabled {
            let step = if state.interlaced { 1 } else { 2 };
            for y in (0..state.height).step_by(step) {
                for x in 0..state.width {
                    let pixel_index = y * state.width + x;
                    let address = state.start_address.wrapping_add((pixel_index * 2) as u32);
                    let pixel = rgb565_to_xrgb8888(self.read_u16(address)?, 0);
                    output[pixel_index] = pixel;
                    if !state.interlaced && y + 1 < state.height {
                        output[pixel_index + state.width] = pixel;
                    }
                }
            }
        }
        apply_frame_fade(output, state.fade);
        Ok((state.width, state.height))
    }

    pub fn drain_audio_samples(&mut self, destination: &mut Vec<i16>) {
        self.devices.drain_audio_samples(destination);
    }

    pub fn drain_uart_output(&mut self, destination: &mut Vec<u8>) {
        self.devices.drain_uart_output(destination);
    }

    fn render_ppu(
        &self,
        output: &mut [u32],
        width: usize,
        height: usize,
    ) -> Result<(), EmulatorError> {
        let state = self.devices.ppu_render_state();
        if !state.enabled {
            return Ok(());
        }

        for depth in 0..4 {
            for layer in &state.layers {
                if layer.control & 8 == 0 || (layer.attribute >> 13) & 3 != depth {
                    continue;
                }
                if layer.control & 1 != 0 {
                    self.render_ppu_bitmap_layer(
                        output,
                        width,
                        height,
                        *layer,
                        state.transparent_rgb,
                        state.blend_subtract,
                    )?;
                } else {
                    self.render_ppu_character_layer(
                        output,
                        width,
                        height,
                        *layer,
                        &state.character_palette,
                        state.blend_subtract,
                    )?;
                }
            }
            if state.sprite_enabled {
                self.render_ppu_sprites(output, width, height, depth, &state)?;
            }
        }
        Ok(())
    }

    fn render_ppu_bitmap_layer(
        &self,
        output: &mut [u32],
        width: usize,
        height: usize,
        layer: crate::video::PpuLayerState,
        transparent_rgb: u32,
        blend_subtract: bool,
    ) -> Result<(), EmulatorError> {
        let position_x = sign_extend_position(layer.position_x, 10);
        let position_y = sign_extend_position(layer.position_y, 9);
        let blend = if layer.control & 0x100 != 0 {
            layer.blend.min(63)
        } else {
            0
        };
        for destination_y in 0..height {
            let source_y = destination_y as i32 + position_y;
            if source_y < 0 {
                continue;
            }
            let line = if layer.control & 4 != 0 {
                0
            } else {
                source_y as u32
            };
            let line_offset = self
                .read_unaligned_u32(layer.number_pointer.wrapping_add(line * 4))?
                .wrapping_mul(2);
            for destination_x in 0..width {
                let source_x = destination_x as i32 + position_x;
                if source_x < 0 {
                    continue;
                }
                let address = layer
                    .buffer_start
                    .wrapping_add(line_offset)
                    .wrapping_add(source_x as u32 * 2);
                let raw = self.read_u16(address)?;
                let color = if layer.control & 0x1000 != 0 {
                    if transparent_rgb & 0x1_0000 != 0 && transparent_rgb as u16 == raw {
                        continue;
                    }
                    rgb565_to_xrgb8888(raw, 0)
                } else {
                    if raw & 0x8000 != 0 {
                        continue;
                    }
                    argb1555_to_xrgb8888(raw)
                };
                let index = destination_y * width + destination_x;
                output[index] = if blend == 0 {
                    color
                } else {
                    blend_pixels(output[index], color, blend, blend_subtract)
                };
            }
        }
        Ok(())
    }

    fn render_ppu_character_layer(
        &self,
        output: &mut [u32],
        width: usize,
        height: usize,
        layer: crate::video::PpuLayerState,
        palette: &[u16],
        blend_subtract: bool,
    ) -> Result<(), EmulatorError> {
        let position_x = sign_extend_position(layer.position_x, 10);
        let position_y = sign_extend_position(layer.position_y, 9);
        let character_width = 8_usize << ((layer.attribute >> 4) & 3);
        let character_height = 8_usize << ((layer.attribute >> 6) & 3);
        let bits_per_pixel = usize::from((((layer.attribute & 3) + 1) << 1) as u8);
        let pixels_per_word = 32 / bits_per_pixel;
        let pen_mask = (1_u32 << bits_per_pixel) - 1;
        let palette_bank = ((layer.attribute >> 8) & 0x1f) as usize * 16;
        let characters_per_line = 1024 / character_width;
        let blend = if layer.control & 0x100 != 0 {
            layer.blend.min(63)
        } else {
            0
        };

        for destination_y in 0..height {
            let source_y = destination_y as i32 + position_y;
            if source_y < 0 {
                continue;
            }
            let tile_y = source_y as usize / character_height;
            let pixel_y = source_y as usize % character_height;
            let map_y = if layer.control & 4 != 0 { 0 } else { tile_y };
            for destination_x in 0..width {
                let source_x = destination_x as i32 + position_x;
                if source_x < 0 {
                    continue;
                }
                let tile_x = source_x as usize / character_width;
                let pixel_x = source_x as usize % character_width;
                let map_index = map_y * characters_per_line + tile_x;
                let character =
                    self.read_u16(layer.number_pointer.wrapping_add((map_index * 2) as u32))?;
                if character == 0 {
                    continue;
                }
                let character_address = layer.buffer_start.wrapping_add(
                    u32::from(character).wrapping_mul((character_width * character_height) as u32),
                );
                let data_address = character_address
                    .wrapping_add((pixel_y * character_width) as u32)
                    .wrapping_add(((pixel_x * bits_per_pixel) / 8) as u32);
                let data = self.read_unaligned_u32(data_address)?;
                let pen =
                    ((data >> ((pixel_x % pixels_per_word) * bits_per_pixel)) & pen_mask) as usize;
                let raw = palette[(palette_bank + pen) % palette.len()];
                if raw & 0x8000 != 0 {
                    continue;
                }
                let color = if layer.control & 0x1000 != 0 {
                    rgb565_to_xrgb8888(raw, 0)
                } else {
                    argb1555_to_xrgb8888(raw)
                };
                let index = destination_y * width + destination_x;
                output[index] = if blend == 0 {
                    color
                } else {
                    blend_pixels(output[index], color, blend, blend_subtract)
                };
            }
        }
        Ok(())
    }

    fn render_ppu_sprites(
        &self,
        output: &mut [u32],
        width: usize,
        height: usize,
        depth: u32,
        state: &crate::video::PpuRenderState,
    ) -> Result<(), EmulatorError> {
        let count = (state.sprite_max + 1).min(state.sprites.len() / 2);
        for index in 0..count {
            let control = state.sprites[index * 2];
            let attribute = state.sprites[index * 2 + 1];
            if (attribute >> 13) & 3 != depth || control & 0xffff == 0 {
                continue;
            }
            let sprite_number = control & 0xffff;
            let position_x = sign_extend_position(control >> 16, 10);
            let position_y = sign_extend_position(attribute >> 16, 10);
            let sprite_width = 8_usize << ((attribute >> 4) & 3);
            let sprite_height = 8_usize << ((attribute >> 6) & 3);
            let bits_per_pixel = usize::from((((attribute & 3) + 1) << 1) as u8);
            let pixels_per_byte = 8 / bits_per_pixel;
            let pen_mask = ((1_u16 << bits_per_pixel) - 1) as u8;
            let palette_bank = ((attribute >> 8) & 0x1f) as usize * 16;
            let flip_x = attribute & 4 != 0;
            let flip_y = attribute & 8 != 0;
            let blend = if attribute & 0x8000 != 0 {
                63 - ((attribute >> 26) & 0x3f) as u8
            } else {
                0
            };
            let bytes_per_sprite = sprite_width * sprite_height * bits_per_pixel / 8;
            let sprite_address = state
                .sprite_buffer_start
                .wrapping_add(sprite_number.wrapping_mul(bytes_per_sprite as u32));

            for y in 0..sprite_height {
                let source_y = if flip_y { sprite_height - 1 - y } else { y };
                let destination_y = position_y + y as i32;
                if !(0..height as i32).contains(&destination_y) {
                    continue;
                }
                for x in 0..sprite_width {
                    let source_x = if flip_x { sprite_width - 1 - x } else { x };
                    let destination_x = position_x + x as i32;
                    if !(0..width as i32).contains(&destination_x) {
                        continue;
                    }
                    let source_pixel = source_y * sprite_width + source_x;
                    let byte = self.read_u8(
                        sprite_address.wrapping_add((source_pixel / pixels_per_byte) as u32),
                    )?;
                    let slot = source_pixel % pixels_per_byte;
                    let shift = 8 - (slot + 1) * bits_per_pixel;
                    let pen = usize::from((byte >> shift) & pen_mask);
                    let raw =
                        state.sprite_palette[(palette_bank + pen) % state.sprite_palette.len()];
                    if raw & 0x8000 != 0 {
                        continue;
                    }
                    let color = argb1555_to_xrgb8888(raw);
                    let output_index = destination_y as usize * width + destination_x as usize;
                    output[output_index] = if blend == 0 {
                        color
                    } else {
                        blend_pixels(output[output_index], color, blend, state.blend_subtract)
                    };
                }
            }
        }
        Ok(())
    }

    fn read_unaligned_u32(&self, address: u32) -> Result<u32, EmulatorError> {
        Ok(u32::from_le_bytes([
            self.read_u8(address)?,
            self.read_u8(address.wrapping_add(1))?,
            self.read_u8(address.wrapping_add(2))?,
            self.read_u8(address.wrapping_add(3))?,
        ]))
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

fn sign_extend_position(value: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((value << shift) as i32) >> shift
}

fn argb1555_to_xrgb8888(value: u16) -> u32 {
    let red = u32::from((value >> 10) & 0x1f) * 255 / 31;
    let green = u32::from((value >> 5) & 0x1f) * 255 / 31;
    let blue = u32::from(value & 0x1f) * 255 / 31;
    0xff00_0000 | (red << 16) | (green << 8) | blue
}

fn blend_pixels(background: u32, foreground: u32, level: u8, subtract: bool) -> u32 {
    let level = i32::from(level);
    let blend_channel = |shift: u32| {
        let background = ((background >> shift) & 0xff) as i32;
        let foreground = ((foreground >> shift) & 0xff) as i32;
        let value = if subtract {
            background * level / 63 - foreground * (63 - level) / 63
        } else {
            background * level / 63 + foreground * (63 - level) / 63
        };
        value.clamp(0, 255) as u32
    };
    0xff00_0000 | (blend_channel(16) << 16) | (blend_channel(8) << 8) | blend_channel(0)
}

fn apply_frame_fade(output: &mut [u32], fade: u8) {
    if fade == 0 {
        return;
    }
    let scale = u32::from(255 - fade);
    for pixel in output {
        let red = ((*pixel >> 16) & 0xff) * scale / 255;
        let green = ((*pixel >> 8) & 0xff) * scale / 255;
        let blue = (*pixel & 0xff) * scale / 255;
        *pixel = 0xff00_0000 | (red << 16) | (green << 8) | blue;
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
    use crate::{
        BIOS_ROM_SIZE, CD_INTERRUPT_SOURCE, CPU_CLOCK_HZ, INTERNAL_ROM_SIZE, RAW_SECTOR_SIZE,
    };

    fn test_bus() -> Bus {
        let mut internal = vec![0; INTERNAL_ROM_SIZE];
        let mut bios = vec![0; BIOS_ROM_SIZE];
        internal[0..4].copy_from_slice(&0x1122_3344_u32.to_le_bytes());
        bios[0..4].copy_from_slice(&0xaabb_ccdd_u32.to_le_bytes());
        Bus::new(Firmware::from_parts(&internal, &bios).unwrap())
    }

    fn test_disc() -> DiscImage {
        let mut image = vec![0; 22 * RAW_SECTOR_SIZE];
        for (lba, sector) in image
            .as_chunks_mut::<RAW_SECTOR_SIZE>()
            .0
            .iter_mut()
            .enumerate()
        {
            sector[..12].copy_from_slice(&[
                0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0,
            ]);
            let frame = lba as u32 + 150;
            let bcd = |value: u32| (((value / 10) << 4) | (value % 10)) as u8;
            sector[12] = bcd(frame / 4_500);
            sector[13] = bcd((frame / 75) % 60);
            sector[14] = bcd(frame % 75);
            sector[15] = 1;
        }
        let descriptor = |image: &mut [u8], lba: usize, kind: u8, id: &[u8; 5]| {
            let start = lba * RAW_SECTOR_SIZE + 16;
            image[start] = kind;
            image[start + 1..start + 6].copy_from_slice(id);
            image[start + 6] = 1;
        };
        descriptor(&mut image, 16, 1, b"CD001");
        descriptor(&mut image, 17, 0xff, b"CD001");
        descriptor(&mut image, 18, 0, b"NSR02");
        DiscImage::from_mode1_2352(image).unwrap()
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
    fn reset_window_starts_on_external_rom() {
        let mut bus = test_bus();

        assert_eq!(bus.read_u32(0x9f00_0000).unwrap(), 0xaabb_ccdd);
        bus.set_boot_source(BootSource::InternalRom);
        assert_eq!(bus.read_u32(0x9f00_0000).unwrap(), 0x1122_3344);
    }

    #[test]
    fn external_rom_pin_selection_switches_the_boot_window() {
        let mut bus = test_bus();
        bus.set_boot_source(BootSource::InternalRom);

        bus.write_u32(0x0820_0004, 1 << 24).unwrap();

        assert_eq!(bus.boot_source(), BootSource::ExternalRom);
        assert_eq!(bus.read_u32(0x9f00_0000).unwrap(), 0xaabb_ccdd);
    }

    #[test]
    fn mmio_subword_accesses_preserve_other_byte_lanes() {
        let mut bus = test_bus();
        bus.write_u32(0x0807_0060, 0x1122_3344).unwrap();

        bus.write_u8(0x0807_0061, 0xaa).unwrap();
        bus.write_u16(0x0807_0062, 0xbbcc).unwrap();

        assert_eq!(bus.read_u32(0x0807_0060).unwrap(), 0xbbcc_aa44);
        assert_eq!(bus.read_u8(0x0807_0061).unwrap(), 0xaa);
        assert_eq!(bus.read_u16(0x0807_0062).unwrap(), 0xbbcc);
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
        bus.set_boot_source(BootSource::InternalRom);

        bus.reset();

        assert_eq!(bus.read_u8(0x100).unwrap(), 0);
        assert_eq!(bus.read_u8(INTERNAL_SRAM_START).unwrap(), 0);
        assert_eq!(bus.boot_source(), BootSource::ExternalRom);
    }

    #[test]
    fn cd_servo_dma_copies_raw_sector_and_raises_interrupt() {
        let disc = test_disc();
        let mut bus = test_bus();
        bus.set_disc(Some(&disc));
        bus.write_u32(0x0806_0048, 0).unwrap();
        bus.write_u32(0x0806_004c, 2).unwrap();
        bus.write_u32(0x0806_0050, 0).unwrap();
        bus.write_u32(0x0806_0060, 0x100).unwrap();
        bus.write_u32(0x0806_0064, 0xa2f).unwrap();
        bus.write_u32(0x0806_0068, 0x100).unwrap();
        bus.write_u32(0x0806_006c, RAW_SECTOR_SIZE as u32).unwrap();
        bus.write_u32(0x0806_0044, 0).unwrap();

        bus.tick(CPU_CLOCK_HZ / 75).unwrap();
        bus.service_cd(&disc).unwrap();

        assert_eq!(bus.read_u32(0x100).unwrap(), 0xffff_ff00);
        assert_eq!(bus.read_u32(0x10c).unwrap(), 0x0100_0200);
        assert_eq!(bus.read_u32(0x0806_0068).unwrap(), 0x100);
        assert_eq!(bus.take_pending_interrupts(), 1_u64 << CD_INTERRUPT_SOURCE);
    }

    #[test]
    fn cd_servo_dma_pads_sixteen_byte_trailer() {
        let disc = test_disc();
        let mut bus = test_bus();
        bus.set_disc(Some(&disc));
        bus.write_u32(0x0806_0048, 0).unwrap();
        bus.write_u32(0x0806_004c, 2).unwrap();
        bus.write_u32(0x0806_0050, 0).unwrap();
        bus.write_u32(0x0806_0060, 0x100).unwrap();
        bus.write_u32(0x0806_0064, 0xa3f).unwrap();
        bus.write_u32(0x0806_0068, 0x100).unwrap();
        bus.write_u32(0x0806_006c, (RAW_SECTOR_SIZE + 16) as u32)
            .unwrap();
        bus.write_u32(0x0806_0044, 0).unwrap();

        bus.tick(CPU_CLOCK_HZ / 75).unwrap();
        bus.service_cd(&disc).unwrap();

        assert_eq!(bus.read_u32(0x100).unwrap(), 0xffff_ff00);
        assert_eq!(bus.read_u32(0xa30).unwrap(), 0);
        assert_eq!(bus.take_pending_interrupts(), 1_u64 << CD_INTERRUPT_SOURCE);
    }

    #[test]
    fn direct_framebuffer_renders_rgb565_and_progressive_line_pairs() {
        let mut bus = test_bus();
        bus.write_u32(0x0807_0000, 0x1000).unwrap();
        bus.write_u32(0x0809_0020, 0).unwrap();
        bus.write_u16(0x1000, 0xf800).unwrap();
        bus.write_u16(0x1002, 0x07e0).unwrap();
        let mut frame = Vec::new();

        let dimensions = bus.render_frame(&mut frame).unwrap();

        assert_eq!(dimensions, (320, 240));
        assert_eq!(frame[0], 0xffff_0000);
        assert_eq!(frame[1], 0xff00_ff00);
        assert_eq!(frame[320], 0xffff_0000);
        assert_eq!(frame[321], 0xff00_ff00);
    }

    #[test]
    fn dac_dma_reads_unsigned_interleaved_pcm() {
        let mut bus = test_bus();
        bus.write_u16(0x2000, 0).unwrap();
        bus.write_u16(0x2002, u16::MAX).unwrap();
        bus.write_u32(0x0821_003c, 3).unwrap();
        bus.write_u32(0x0805_1474, !3).unwrap();
        bus.write_u32(0x0805_1080, 0x2000).unwrap();
        bus.write_u32(0x0805_1084, 0).unwrap();
        bus.write_u32(0x0805_1064, 1_223).unwrap();
        bus.write_u32(0x0805_1088, 0x4007).unwrap();
        bus.write_u32(0x0805_1034, 0x1003).unwrap();

        bus.tick(2_448).unwrap();
        let mut samples = Vec::new();
        bus.drain_audio_samples(&mut samples);

        assert_eq!(samples, [i16::MIN, i16::MAX]);
        assert_eq!(bus.read_u32(0x0805_1040).unwrap(), 0);
        assert_eq!(bus.read_u32(0x0805_1044).unwrap(), u32::from(u16::MAX));
    }

    #[test]
    fn ppu_bitmap_layer_uses_line_table_and_rgb565_pixels() {
        let mut bus = test_bus();
        bus.write_u32(0x0801_0000, 0x1000).unwrap();
        bus.write_u32(0x0801_002c, 0x1009).unwrap();
        bus.write_u32(0x0801_0030, 0x3000).unwrap();
        bus.write_u32(0x0801_00a0, 0x4000).unwrap();
        bus.write_u32(0x3000, 0).unwrap();
        bus.write_u16(0x4000, 0xf800).unwrap();
        bus.write_u16(0x4002, 0x001f).unwrap();
        let mut frame = Vec::new();

        bus.render_frame(&mut frame).unwrap();

        assert_eq!(frame[0], 0xffff_0000);
        assert_eq!(frame[1], 0xff00_00ff);
    }

    #[test]
    fn ppu_character_layer_resolves_number_and_palette_entries() {
        let mut bus = test_bus();
        bus.write_u32(0x0801_0000, 0x1000).unwrap();
        bus.write_u32(0x0801_002c, 0x0008).unwrap();
        bus.write_u32(0x0801_0030, 0x3000).unwrap();
        bus.write_u32(0x0801_00a0, 0x4000).unwrap();
        bus.write_u32(0x0801_1004, 0x7c00).unwrap();
        bus.write_u16(0x3000, 1).unwrap();
        bus.write_u8(0x4040, 1).unwrap();
        let mut frame = Vec::new();

        bus.render_frame(&mut frame).unwrap();

        assert_eq!(frame[0], 0xffff_0000);
        assert_eq!(frame[1], 0xff00_0000);
    }

    #[test]
    fn ppu_sprite_uses_descriptor_pattern_and_sprite_palette() {
        let mut bus = test_bus();
        bus.write_u32(0x0801_0000, 0x1000).unwrap();
        bus.write_u32(0x0801_0004, 1).unwrap();
        bus.write_u32(0x0801_0008, 0).unwrap();
        bus.write_u32(0x0801_00d0, 0x5000).unwrap();
        bus.write_u32(0x0801_1804, 0x03e0).unwrap();
        bus.write_u32(0x0801_4000, 1).unwrap();
        bus.write_u32(0x0801_4004, 0).unwrap();
        bus.write_u8(0x5010, 0x40).unwrap();
        let mut frame = Vec::new();

        bus.render_frame(&mut frame).unwrap();

        assert_eq!(frame[0], 0xff00_ff00);
        assert_eq!(frame[1], 0xff00_0000);
    }

    #[test]
    fn ppu_color_helpers_handle_alpha_blending_and_fade() {
        assert_eq!(argb1555_to_xrgb8888(0x7c00), 0xffff_0000);
        assert_eq!(
            blend_pixels(0xff00_0000, 0xffff_ffff, 0, false),
            0xffff_ffff
        );
        let mut frame = [0xffff_8040];
        apply_frame_fade(&mut frame, 255);
        assert_eq!(frame, [0xff00_0000]);
    }
}
