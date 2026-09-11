use crate::{CPU_CLOCK_HZ, PERIPHERAL_CLOCK_HZ};

pub(crate) const PPU_BASE: u32 = 0x0801_0000;
pub(crate) const PPU_END: u32 = 0x0801_ffff;
pub(crate) const TVE_CONTROL: u32 = 0x0803_0000;
pub(crate) const TVE_FADE: u32 = 0x0803_000c;
pub(crate) const TV_BUFFER_START: u32 = 0x0807_0000;
pub(crate) const MIU_STATUS: u32 = 0x0807_006c;
pub(crate) const TV_BUFFER_CONTROL: u32 = 0x0809_0020;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DirectFrameState {
    pub width: usize,
    pub height: usize,
    pub interlaced: bool,
    pub enabled: bool,
    pub start_address: u32,
    pub fade: u8,
}

#[derive(Debug, Clone)]
pub(crate) struct VideoController {
    ppu_registers: [u32; 64],
    ppu_memory: Box<[u8]>,
    irq_control: u32,
    irq_status: u32,
    tve_control: u32,
    fade: u8,
    buffer_start: [u32; 3],
    buffer_control: u8,
    frame_cycle: u64,
}

impl Default for VideoController {
    fn default() -> Self {
        Self {
            ppu_registers: [0; 64],
            ppu_memory: vec![0; 0x5_000].into_boxed_slice(),
            irq_control: 0,
            irq_status: 0,
            tve_control: 0,
            fade: 0,
            buffer_start: [0; 3],
            buffer_control: 3,
            frame_cycle: 0,
        }
    }
}

impl VideoController {
    pub(crate) fn read(&self, address: u32) -> Option<u32> {
        match address {
            PPU_BASE..=PPU_END => self.read_ppu(address - PPU_BASE),
            TVE_CONTROL => Some(self.tve_control),
            TVE_FADE => Some(u32::from(self.fade)),
            TV_BUFFER_START..=0x0807_000b => {
                Some(self.buffer_start[((address - TV_BUFFER_START) / 4) as usize])
            }
            MIU_STATUS => Some(1),
            TV_BUFFER_CONTROL => Some(u32::from(self.buffer_control)),
            _ => None,
        }
    }

    pub(crate) fn write(&mut self, address: u32, value: u32) -> Option<()> {
        match address {
            PPU_BASE..=PPU_END => return self.write_ppu(address - PPU_BASE, value),
            TVE_CONTROL => self.tve_control = value,
            TVE_FADE => self.fade = value as u8,
            TV_BUFFER_START..=0x0807_000b => {
                self.buffer_start[((address - TV_BUFFER_START) / 4) as usize] = value;
            }
            TV_BUFFER_CONTROL => self.buffer_control = (value & 3) as u8,
            _ => return None,
        }
        Some(())
    }

    pub(crate) fn tick(&mut self, mut cycles: u64) -> bool {
        let mut raised = false;
        while cycles > 0 {
            let frame_cycles = self.frame_cycles();
            let next = (self.frame_cycle + cycles).min(frame_cycles);
            let blank_start = self.vblank_start_cycle();
            if self.frame_cycle < blank_start && next >= blank_start {
                self.irq_status |= 1;
                raised |= self.irq_control & 1 != 0;
            }
            let consumed = next - self.frame_cycle;
            self.frame_cycle = next;
            cycles -= consumed;
            if self.frame_cycle == frame_cycles {
                self.irq_status |= 2;
                raised |= self.irq_control & 2 != 0;
                self.frame_cycle = 0;
            }
        }
        raised
    }

    pub(crate) fn interrupt_pending(&self) -> bool {
        self.irq_status & self.irq_control & 7 != 0
    }

    pub(crate) fn cycles_until_frame_end(&self) -> u64 {
        self.frame_cycles() - self.frame_cycle
    }

    pub(crate) fn direct_frame_state(&self) -> DirectFrameState {
        let (width, height) = self.dimensions();
        DirectFrameState {
            width,
            height,
            interlaced: self.tve_control & 1 != 0,
            enabled: self.buffer_control < 3,
            start_address: self
                .buffer_start
                .get(usize::from(self.buffer_control))
                .copied()
                .unwrap_or(0),
            fade: self.fade,
        }
    }

    fn dimensions(&self) -> (usize, usize) {
        match self.tve_control & 0x0c {
            0 => (320, 240),
            4 => (640, 480),
            8 => (640, 240),
            _ => (640, 480),
        }
    }

    fn timing(&self) -> (u64, u64) {
        if self.tve_control & 2 != 0 {
            (864, 625)
        } else {
            (858, 525)
        }
    }

    fn frame_cycles(&self) -> u64 {
        let (horizontal, vertical) = self.timing();
        let fields = if self.tve_control & 1 != 0 { 2 } else { 1 };
        horizontal * vertical * fields * (CPU_CLOCK_HZ / PERIPHERAL_CLOCK_HZ)
    }

    fn vblank_start_cycle(&self) -> u64 {
        let (horizontal, _) = self.timing();
        let (_, height) = self.dimensions();
        horizontal * height as u64 * (CPU_CLOCK_HZ / PERIPHERAL_CLOCK_HZ)
    }

    fn read_ppu(&self, offset: u32) -> Option<u32> {
        match offset {
            0x84 => Some(self.irq_status),
            0x94 => {
                let (horizontal, _) = self.timing();
                let line_cycles = horizontal * (CPU_CLOCK_HZ / PERIPHERAL_CLOCK_HZ);
                let line = self.frame_cycle / line_cycles;
                Some(self.ppu_registers[0x90 / 4] + line as u32)
            }
            0x00..=0xff if offset.is_multiple_of(4) => {
                Some(self.ppu_registers[(offset / 4) as usize])
            }
            _ if ppu_memory_offset(offset).is_some() => {
                let index = ppu_memory_offset(offset).unwrap();
                Some(u32::from_le_bytes(
                    self.ppu_memory[index..index + 4].try_into().unwrap(),
                ))
            }
            _ => None,
        }
    }

    fn write_ppu(&mut self, offset: u32, value: u32) -> Option<()> {
        match offset {
            0x80 => {
                self.irq_control = value;
                self.irq_status &= value;
                self.ppu_registers[0x80 / 4] = value;
            }
            0x84 => self.irq_status &= !value,
            0x00..=0xff if offset.is_multiple_of(4) => {
                self.ppu_registers[(offset / 4) as usize] = value;
            }
            _ if ppu_memory_offset(offset).is_some() => {
                let index = ppu_memory_offset(offset).unwrap();
                self.ppu_memory[index..index + 4].copy_from_slice(&value.to_le_bytes());
            }
            _ => return None,
        }
        Some(())
    }
}

fn ppu_memory_offset(offset: u32) -> Option<usize> {
    let valid = matches!(
        offset,
        0x1000..=0x17ff
            | 0x1800..=0x1fff
            | 0x2000..=0x27ff
            | 0x3000..=0x37ff
            | 0x4000..=0x4fff
    );
    if valid && offset.is_multiple_of(4) {
        Some(offset as usize)
    } else {
        None
    }
}

pub(crate) fn rgb565_to_xrgb8888(value: u16, fade: u8) -> u32 {
    let red = u32::from((value >> 11) & 0x1f) * 255 / 31;
    let green = u32::from((value >> 5) & 0x3f) * 255 / 63;
    let blue = u32::from(value & 0x1f) * 255 / 31;
    let scale = u32::from(255 - fade);
    0xff00_0000 | ((red * scale / 255) << 16) | ((green * scale / 255) << 8) | (blue * scale / 255)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_modes_select_expected_geometry_and_timing() {
        let mut video = VideoController::default();
        assert_eq!(
            (
                video.direct_frame_state().width,
                video.direct_frame_state().height
            ),
            (320, 240)
        );
        video.write(TVE_CONTROL, 4).unwrap();
        assert_eq!(
            (
                video.direct_frame_state().width,
                video.direct_frame_state().height
            ),
            (640, 480)
        );
        assert_eq!(video.frame_cycles(), 858 * 525 * 4);
        video.write(TVE_CONTROL, 0x0b).unwrap();
        assert_eq!(
            (
                video.direct_frame_state().width,
                video.direct_frame_state().height
            ),
            (640, 240)
        );
        assert_eq!(video.frame_cycles(), 864 * 625 * 2 * 4);
    }

    #[test]
    fn vblank_status_and_acknowledge_are_cycle_driven() {
        let mut video = VideoController::default();
        video.write(PPU_BASE + 0x80, 3).unwrap();
        let blank_start = video.vblank_start_cycle();
        assert!(!video.tick(blank_start - 1));
        assert!(video.tick(1));
        assert_eq!(video.read(PPU_BASE + 0x84).unwrap(), 1);
        video.write(PPU_BASE + 0x84, 1).unwrap();
        assert!(!video.interrupt_pending());
        assert!(video.tick(video.cycles_until_frame_end()));
        assert_eq!(video.read(PPU_BASE + 0x84).unwrap(), 2);
    }

    #[test]
    fn rgb565_conversion_applies_fade() {
        assert_eq!(rgb565_to_xrgb8888(0xf800, 0), 0xffff_0000);
        assert_eq!(rgb565_to_xrgb8888(0x07e0, 255), 0xff00_0000);
    }
}
