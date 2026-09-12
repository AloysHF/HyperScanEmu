use std::env;
use std::error::Error;
use std::fs;

use hyperscanemu_core::{Emulator, Firmware, StepOutcome};
use hyperscanemu_media::load_disc;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("inspect-disc") {
        if args.len() != 2 {
            return Err(usage().into());
        }
        let disc = load_disc(&args[1])?;
        println!(
            "validated MODE1/2352 image: {} sectors, fingerprint {:016x}",
            disc.sector_count(),
            disc.fingerprint()
        );
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("run") {
        if !(4..=6).contains(&args.len()) {
            return Err(usage().into());
        }
        let frames = args
            .get(4)
            .map(|value| value.parse::<u64>())
            .transpose()?
            .unwrap_or(1);
        let firmware = Firmware::from_parts(&fs::read(&args[1])?, &fs::read(&args[2])?)?;
        let disc = load_disc(&args[3])?;
        let mut emulator = Emulator::new(firmware);
        emulator.attach_disc(disc);
        for _ in 0..frames {
            emulator.run_frame()?;
        }
        print_uart(&mut emulator);
        let (width, height) = emulator.display_size();
        println!(
            "completed {} frame(s): {}x{}, frame fingerprint {:016x}, PC {:08x}, PPU {:08x}, TVE {:08x}",
            frames,
            width,
            height,
            frame_fingerprint(emulator.framebuffer()),
            emulator.cpu().pc(),
            emulator.bus().read_u32(0x0801_0000)?,
            emulator.bus().read_u32(0x0803_0000)?
        );
        println!(
            "video state: sprites {:08x}, layers [{:08x}, {:08x}, {:08x}], TV buffer {}",
            emulator.bus().read_u32(0x0801_0004)?,
            emulator.bus().read_u32(0x0801_002c)?,
            emulator.bus().read_u32(0x0801_0048)?,
            emulator.bus().read_u32(0x0801_0064)?,
            emulator.bus().read_u32(0x0809_0020)?
        );
        if let Some(path) = args.get(5) {
            fs::write(path, encode_ppm(emulator.framebuffer(), width, height))?;
            println!("wrote frame to {path}");
        }
        return Ok(());
    }
    if args.first().map(String::as_str) != Some("trace") || !(3..=4).contains(&args.len()) {
        return Err(usage().into());
    }

    let steps = args
        .get(3)
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(1_000);
    let internal_rom = fs::read(&args[1])?;
    let bios_rom = fs::read(&args[2])?;
    let firmware = Firmware::from_parts(&internal_rom, &bios_rom)?;
    let mut emulator = Emulator::new(firmware);
    let start_pc = emulator.cpu().pc();
    let start_cycles = emulator.cpu().cycles();
    let mut exceptions = 0;
    for executed in 0..steps {
        match emulator.step() {
            Ok(StepOutcome::Exception { .. }) => exceptions += 1,
            Ok(_) => {}
            Err(error) => {
                print_uart(&mut emulator);
                let registers = (0..32)
                    .filter_map(|index| {
                        let value = emulator.cpu().register(index)?;
                        (value != 0).then_some(format!("r{index}={value:08x}"))
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if !registers.is_empty() {
                    eprintln!("registers: {registers}");
                }
                return Err(format!(
                    "trace stopped after {executed} instructions at PC {:08x}: {error}",
                    emulator.cpu().pc()
                )
                .into());
            }
        }
    }
    print_uart(&mut emulator);
    println!(
        "executed {} instructions / {} estimated cycles: PC {:08x} -> {:08x}, {} exceptions, boot {:?}",
        steps,
        emulator.cpu().cycles().wrapping_sub(start_cycles),
        start_pc,
        emulator.cpu().pc(),
        exceptions,
        emulator.bus().boot_source()
    );
    Ok(())
}

fn print_uart(emulator: &mut Emulator) {
    let output = emulator.drain_uart_output();
    if !output.is_empty() {
        eprintln!("UART: {}", String::from_utf8_lossy(&output));
    }
}

fn usage() -> &'static str {
    "usage:\n  hyperscanemu inspect-disc <media.bin|media.cue|media.zip>\n  hyperscanemu trace <internal-rom.bin> <bios.bin> [steps]\n  hyperscanemu run <internal-rom.bin> <bios.bin> <media.bin|media.cue|media.zip> [frames] [frame.ppm]"
}

fn encode_ppm(frame: &[u32], width: usize, height: usize) -> Vec<u8> {
    let mut output = format!("P6\n{width} {height}\n255\n").into_bytes();
    output.reserve(frame.len() * 3);
    for pixel in frame {
        output.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
    }
    output
}

fn frame_fingerprint(frame: &[u32]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x100_0000_01b3;
    frame.iter().fold(OFFSET_BASIS, |hash, pixel| {
        pixel.to_le_bytes().iter().fold(hash, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_fingerprint_is_deterministic_and_content_sensitive() {
        assert_eq!(frame_fingerprint(&[1, 2]), frame_fingerprint(&[1, 2]));
        assert_ne!(frame_fingerprint(&[1, 2]), frame_fingerprint(&[2, 1]));
    }

    #[test]
    fn ppm_encoder_writes_rgb_pixels() {
        assert_eq!(
            encode_ppm(&[0xff12_3456], 1, 1),
            b"P6\n1 1\n255\n\x12\x34\x56"
        );
    }
}
